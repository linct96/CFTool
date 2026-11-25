import io
import json
import tempfile
from contextlib import redirect_stderr
import unittest
from pathlib import Path
from subprocess import CompletedProcess
from unittest.mock import patch

from cftool import candidates, cidr_route_samples, endpoints, node, probe, scan, warn_tunnel_route

LINK = ('vless://00000000-0000-4000-8000-000000000001@edge.example:8443?'
        'type=ws&security=tls&sni=tls.example&host=ws.example&path=%2Fa%3Fb%3Dc#demo')


class Tests(unittest.TestCase):
    @patch('cftool.subprocess.run')
    def test_scan_reads_stdout_without_writing_cfst_csv(self, run):
        run.return_value = CompletedProcess([], 0, '# XIU2/CloudflareSpeedTest v2.3.5\n'
            'IP 地址           已发送  已接收  丢包率  平均延迟  下载速度(MB/s)  地区码\n'
            '1.1.1.1            4       4       0.00    5.60      0.00            N/A\n'
            '2606:4700::1111    4       3       0.25    9.12      10.50           HKG\n')
        self.assertEqual(scan(Path('/unused'), {2053: Path('ips.txt')}, ['-dd']),
                         [(9.12, '2606:4700::1111', 2053, 4, 3, 0.25, 10.5),
                          (5.6, '1.1.1.1', 2053, 4, 4, 0, 0)])
        args = run.call_args.args[0]
        self.assertEqual(args[args.index('-o') + 1], '')
        self.assertGreater(int(args[args.index('-p') + 1]), 10)

    def test_candidates_distinguishes_no_results_from_unknown_output(self):
        self.assertEqual(candidates('[信息] 完整测速结果 IP 数量为 0，跳过输出结果。\n', 443), [])
        with self.assertRaisesRegex(RuntimeError, '未找到 CFST 测速结果表'):
            candidates('unexpected output', 443)

    @patch('cftool.subprocess.run')
    def test_mixed_sources(self, fetch):
        fetch.return_value = CompletedProcess([], 0, b'1.1.1.1:443#dup\n1.1.1.1:8443\n')
        self.assertEqual(endpoints(['1.1.1.1#备注', 'https://example.org/list'], 443),
                         {443: {'1.1.1.1'}, 8443: {'1.1.1.1'}})
        self.assertEqual(endpoints(['2001:db8::1', '[2001:db8::2]:8443'], 443),
                         {443: {'2001:db8::1'}, 8443: {'2001:db8::2'}})
        fetch.assert_called_once()

    @patch('cftool.subprocess.run')
    def test_subscriptions_cannot_load_another_subscription(self, fetch):
        fetch.return_value = CompletedProcess([], 0, b'https://example.org/nested\n')
        with self.assertRaisesRegex(ValueError, '订阅第 1 行不是有效候选地址'):
            endpoints(['https://example.org/list'], 443)
        fetch.assert_called_once()

    def test_parse_ech(self):
        ech = '&ech=cloudflare-ech.com%2Bhttps%3A%2F%2Fdns.alidns.com%2Fdns-query'
        self.assertEqual(node(LINK.replace('#', ech + '#'))[2]['tlsSettings']['echConfigList'],
                         'cloudflare-ech.com+https://dns.alidns.com/dns-query')
        self.assertNotIn('echConfigList', node(LINK)[2]['tlsSettings'])
        for query in ('&ech=', '&ech=%00', '&ech=a&ech=b'):
            with self.subTest(query=query), self.assertRaises(ValueError):
                node(LINK.replace('#', query + '#'))

    def test_reject_unsupported(self):
        for link in (LINK.replace('type=ws', 'type=grpc'), LINK.replace('#', '&allowInsecure=1#'), LINK+'\n'+LINK):
            with self.subTest(link=link), self.assertRaises(ValueError):
                node(link)

    def test_cidr_route_samples(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'ips.txt'
            path.write_text('203.0.113.0/24\n\n2001:db8::/126\n203.0.113.9\ninvalid\n')
            self.assertEqual(list(cidr_route_samples(path)), ['203.0.113.128', '2001:db8::2', '203.0.113.9'])

    @patch('cftool.sys.platform', 'darwin')
    @patch('cftool.subprocess.run')
    def test_tunnel_route_warning_checks_candidates(self, run):
        run.side_effect = [CompletedProcess([], 0, '  interface: en0\n'),
                           CompletedProcess([], 0, '  interface: utun4\n')]
        with redirect_stderr(io.StringIO()) as stderr:
            warn_tunnel_route(['203.0.113.1', '2001:db8::1', '203.0.113.2'])
        self.assertIn('2001:db8::1 的系统路由经过隧道接口 utun4', stderr.getvalue())
        self.assertEqual([call.args[0][3:] for call in run.call_args_list],
                         [['-inet', '203.0.113.1'], ['-inet6', '2001:db8::1']])

    @patch('cftool.sys.platform', 'darwin')
    @patch('cftool.subprocess.run')
    def test_route_warning_distinguishes_physical_interface_from_unknown(self, run):
        for status, output in [(0, '  interface: en0\n'), (1, '')]:
            run.return_value = CompletedProcess([], status, output)
            with redirect_stderr(io.StringIO()) as stderr:
                warn_tunnel_route(['203.0.113.1'])
            if status:
                self.assertIn('无法确认', stderr.getvalue())
            else:
                self.assertEqual(stderr.getvalue(), '')

    @patch('cftool.socket.create_connection')
    @patch('cftool.subprocess.Popen')
    @patch('cftool.subprocess.run')
    def test_probe(self, run, popen, connect):
        popen.return_value.poll.return_value = None
        run.side_effect = [CompletedProcess([], rc, data) for rc, data in
                           [(0, '204 0.123'), (0, '302 0.05'), (28, '204 8')]]
        with tempfile.TemporaryDirectory() as d:
            config = Path(d) / 'config.json'
            samples = probe('xray', config, node(LINK), '192.0.2.1', 2053, 3, 'https://test.example/')
            out = json.loads(config.read_text())['outbounds'][0]
        self.assertEqual(samples, [123])
        peer = out['settings']['vnext'][0]
        self.assertEqual((peer['address'], peer['port']), ('192.0.2.1', 2053))
        stream = out['streamSettings']
        self.assertEqual(stream['tlsSettings']['serverName'], 'tls.example')
        self.assertEqual(stream['wsSettings'], {'host': 'ws.example', 'path': '/a?b=c'})
        self.assertIn('socks5h://127.0.0.1:', str(run.call_args))
        popen.return_value.terminate.assert_called_once()
