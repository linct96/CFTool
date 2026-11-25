#!/usr/bin/env python3
"""本地 CFST 测速，可选 Xray VLESS/WS/TLS 复测。"""
import argparse
import csv
import ipaddress
import json
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path
from urllib.parse import parse_qsl, urlsplit

ROOT = Path(__file__).resolve().parent


def node(link):
    if any(c.isspace() for c in link.strip()):
        raise ValueError('链接不能含空白字符')
    u = urlsplit(link.strip())
    if u.scheme != 'vless' or not u.hostname or not u.username or u.password or u.path:
        raise ValueError('需要单条 vless:// 分享链接')
    uuid.UUID(u.username)
    pairs = parse_qsl(u.query, keep_blank_values=True, strict_parsing=True)
    q = dict(pairs)
    if len(q) != len(pairs) or q.keys() - {'type', 'security', 'encryption', 'host', 'path', 'sni', 'fp', 'alpn', 'flow', 'ech'}:
        raise ValueError('链接有重复或未支持的参数，不进行失真的测试')
    if (q.get('type'), q.get('security'), q.get('encryption', 'none'), q.get('flow', '')) != ('ws', 'tls', 'none', ''):
        raise ValueError('当前仅支持 VLESS + WS + TLS（无 flow）')
    port = 443 if u.port is None else u.port
    if not port:
        raise ValueError('端口不能为 0')
    tls = {'serverName': q.get('sni') or u.hostname}
    if q.get('fp'):
        tls['fingerprint'] = q['fp']
    if q.get('alpn'):
        tls['alpn'] = q['alpn'].split(',')
    if 'ech' in q:
        ech = q['ech']
        if not ech or any(c.isspace() or ord(c) < 32 or 127 <= ord(c) <= 159 for c in ech):
            raise ValueError('ECH 参数不能为空或包含空白、控制字符')
        tls['echConfigList'] = ech
    stream = {'network': 'ws', 'security': 'tls', 'tlsSettings': tls,
              'wsSettings': {'host': q.get('host') or tls['serverName'], 'path': q.get('path') or '/'}}
    return u.username, port, stream


def endpoints(lines, default_port):
    groups = {}

    def parse(lines, subscription=False):
        for number, line in enumerate(lines, 1):
            entry = line.partition('#')[0].strip()
            if not entry:
                continue
            if not subscription and entry.startswith('https://'):
                r = subprocess.run(['curl', '-q', '-fsSL', '--proto', '=https', '--proto-redir', '=https',
                                    '--max-time', '30', '--max-filesize', '2097152', '--', entry], capture_output=True)
                if r.returncode:
                    raise RuntimeError(f'第 {number} 行订阅下载失败 (curl {r.returncode})')
                parse(r.stdout.decode('utf-8-sig').splitlines(), True)
                continue
            try:
                port = default_port
                try:
                    ip = ipaddress.ip_address(entry)
                except ValueError:
                    addr = urlsplit('//' + entry)
                    ip = ipaddress.ip_address(addr.hostname)
                    if addr.username is not None or addr.path or addr.query or addr.port == 0 or entry.endswith(':'):
                        raise ValueError
                    port = addr.port or default_port
                if '%' in str(ip) or (subscription and not ip.is_global):
                    raise ValueError
                groups.setdefault(port, set()).add(str(ip))
            except ValueError:
                source = '订阅' if subscription else '候选文件'
                raise ValueError(f'{source}第 {number} 行不是有效候选地址') from None

    parse(lines)
    return groups


def cidr_route_samples(path):
    # shortcut: one representative per CIDR; check exact addresses when CFST exposes its sampled IPs.
    for line in path.read_text(encoding='utf-8-sig').splitlines():
        try:
            net = ipaddress.ip_network(line.strip(), strict=False)
        except ValueError:
            continue
        yield str(net[net.num_addresses // 2])


def warn_tunnel_route(addresses):
    if sys.platform != 'darwin':
        return
    incomplete = False
    for ip in addresses:
        family = '-inet6' if ':' in ip else '-inet'
        try:
            route = subprocess.run(['/sbin/route', '-n', 'get', family, ip],
                                   capture_output=True, text=True, timeout=1)
        except (OSError, subprocess.TimeoutExpired):
            incomplete = True
            break
        interface = next((line.split(':', 1)[1].strip() for line in route.stdout.splitlines()
                          if line.lstrip().startswith('interface:')), '')
        if route.returncode or not interface:
            incomplete = True
        elif interface.startswith(('utun', 'tun', 'tap', 'ppp', 'ipsec')):
            print(f'测速前提示：{ip} 的系统路由经过隧道接口 {interface}（TUN/VPN）。'
                  '结果不一定代表直连；若要测直连，请关闭 TUN/VPN 或排除候选 IP 后重试。',
                  file=sys.stderr, flush=True)
            return
    if incomplete:
        print('测速前提示：部分候选 IP 的系统路由无法确认，测速结果不一定代表直连。',
              file=sys.stderr, flush=True)


def candidates(output, port):
    # shortcut: parse the bundled CFST v2.3.5 table until CFST supports structured stdout.
    lines = iter(output.splitlines())
    for line in lines:
        if line.startswith('IP 地址'):
            break
        if line.strip() == '[信息] 完整测速结果 IP 数量为 0，跳过输出结果。':
            return []
    else:
        raise RuntimeError('未找到 CFST 测速结果表')

    rows = []
    for line in lines:
        if line.strip():
            ip, sent, received, loss, delay, speed, *_ = line.split()
            rows.append((float(delay), str(ipaddress.ip_address(ip)), port,
                         int(sent), int(received), float(loss), float(speed)))
    return rows


def endpoint_label(ip, port):
    return f'[{ip}]:{port}' if ':' in ip else f'{ip}:{port}'


def scan(tmp, inputs, options):
    ips = []
    for port, source in inputs.items():
        result = subprocess.run([ROOT.parent / 'src-tauri/resources/bin/cfst', '-f', source, '-tp', str(port),
                                 '-o', '', '-p', str(sys.maxsize), *options],
                                check=True, cwd=tmp, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, encoding='utf-8')
        ips.extend(candidates(result.stdout, port))
    return sorted(ips, key=lambda row: (-row[6], row[0], row[1], row[2]))


def candidate_files(tmp, groups):
    inputs = {}
    for port, addresses in groups.items():
        inputs[port] = tmp / f'{port}.txt'
        inputs[port].write_text('\n'.join(sorted(addresses)) + '\n')
    return inputs


def probe(xray, config, parsed, ip, port, rounds, url):
    user, _, stream = parsed
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        local = sock.getsockname()[1]
    config.write_text(json.dumps({'log': {'loglevel': 'none'}, 'inbounds': [
        {'listen': '127.0.0.1', 'port': local, 'protocol': 'socks'}], 'outbounds': [
        {'protocol': 'vless', 'settings': {'vnext': [{'address': ip, 'port': port,
         'users': [{'id': user, 'encryption': 'none'}]}]}, 'streamSettings': stream}]}))
    proc = subprocess.Popen([xray, 'run', '-c', config], cwd=config.parent,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        deadline = time.monotonic() + 5
        while True:
            if proc.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError('Xray 启动失败，请检查版本和节点参数')
            try:
                with socket.create_connection(('127.0.0.1', local), timeout=.1):
                    break
            except OSError:
                time.sleep(.05)
        samples = []
        for _ in range(rounds):
            r = subprocess.run(['curl', '-q', '-s', '--noproxy', '', '--proxy', f'socks5h://127.0.0.1:{local}',
                                '--max-time', '8', '-o', '/dev/null', '-w', '%{http_code} %{time_total}', url],
                               capture_output=True, text=True)
            if proc.poll() is not None:
                raise RuntimeError('测试中 Xray 意外退出')
            if not r.returncode:
                status, seconds = r.stdout.split()
                if status == '204':
                    samples.append(float(seconds) * 1000)
        return samples
    finally:
        if proc.poll() is None:
            proc.terminate()
        try:
            proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()


def retest(xray, tmp, parsed, ips, top, rounds, url, log):
    results = []
    for _, ip, port, *_ in ips[:top]:
        endpoint = endpoint_label(ip, port)
        log(f'Xray 复测 {endpoint} …')
        samples = probe(xray, tmp / 'config.json', parsed, ip, port, rounds, url)
        results.append((-len(samples), statistics.median(samples) if samples else float('inf'), endpoint))
    return sorted(results)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--node', type=Path, help='可选：仅含一条 VLESS 分享链接的本地文件')
    p.add_argument('--xray', default='xray', help='Xray-core 可执行文件')
    sources = p.add_mutually_exclusive_group()
    sources.add_argument('--file', type=Path, help='每行一个 IP[:端口][#备注] 或 HTTPS 订阅地址')
    sources.add_argument('--ips', type=Path, default=ROOT.parent / 'src/assets/ip.txt', help='CFST IP 段文件')
    p.add_argument('--top', type=int, default=10, help='复测候选数，默认 10')
    p.add_argument('--rounds', type=int, default=5, help='每个 IP 请求次数，默认 5')
    p.add_argument('--url', default='https://www.gstatic.com/generate_204', help='预期返回 204 的 HTTPS 地址')
    a = p.parse_args()
    if a.node and (min(a.top, a.rounds) < 1 or not a.url.startswith('https://')):
        p.error('top、rounds 必须为正数，url 必须为 HTTPS')
    if a.file and not a.file.is_file():
        p.error(f'找不到候选文件: {a.file}')
    try:
        parsed = node(a.node.read_text().strip()) if a.node else None
    except ValueError:
        p.error('无效或未支持的分享链接；请参阅 README 中支持的参数')
    xray = shutil.which(a.xray) if parsed else None
    if parsed and (not xray or not shutil.which('curl')):
        p.error('复测需要 curl 和 Xray-core；可用 --xray 指定可执行文件路径')
    port = parsed[1] if parsed else 443
    with tempfile.TemporaryDirectory(prefix='cftool-') as directory:
        tmp = Path(directory)
        if a.file:
            groups = endpoints(a.file.read_text(encoding='utf-8-sig').splitlines(), port)
            print(f'解析得到 {sum(map(len, groups.values()))} 个候选，{len(groups)} 个端口', flush=True)
            warn_tunnel_route(sorted(set().union(*groups.values())))
            inputs = candidate_files(tmp, groups)
        else:
            inputs = {port: a.ips.resolve()}
            warn_tunnel_route(cidr_route_samples(a.ips))
        ips = scan(tmp, inputs, ['-dd', '-tlr', '0'])
        if not ips:
            raise RuntimeError('CFST 未找到无丢包的候选 IP')
        if not parsed:
            with Path('result.csv').open('w', encoding='utf-8-sig', newline='') as f:
                writer = csv.writer(f)
                writer.writerow(['入口', '成功次数', '测试次数', '丢包率', '平均延迟(ms)'])
                for delay, ip, port, sent, received, loss, _ in ips:
                    writer.writerow([endpoint_label(ip, port), received, sent, loss, delay])
            print('CFST 结果已保存到 ./result.csv（未启用 Xray）')
            return
        results = retest(Path(xray).resolve(), tmp, parsed, ips, a.top, a.rounds, a.url, print)
        with Path('result.csv').open('w', encoding='utf-8-sig', newline='') as f:
            writer = csv.writer(f)
            writer.writerow(['入口', '成功次数', '测试次数', 'HTTPS 耗时中位数(ms)'])
            for count, ms, endpoint in results:
                writer.writerow([endpoint, -count, a.rounds, round(ms) if count else ''])
        print('复测结果已保存到 ./result.csv')
        count, _, best = results[0]
        if not count:
            raise RuntimeError('没有候选完成 HTTPS 204 请求，请检查节点和测试地址')
        print('本轮推荐:', best)


def interrupted(_signum, _frame):
    raise KeyboardInterrupt


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, interrupted)
    try:
        main()
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as e:
        sys.exit(str(e))
    except KeyboardInterrupt:
        sys.exit(130)
