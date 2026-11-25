# cftool CLI

> 此 Python CLI 已停止维护，仅作归档。Debian 支持面向[桌面版](../README.md)。

CFST 延迟测速，可选用 Xray 为 VLESS + WebSocket + TLS 节点复测入口 IP。不会改 hosts、系统代理或节点配置。

[桌面版](../README.md) 位于项目根目录，也支持这两种模式。

## 运行（macOS Apple Silicon）

以下命令均在 `other/` 目录运行。

需要 Python 3.8+；CLI 和桌面版共用 `../src-tauri/resources/bin/` 下的 macOS arm64 二进制。仅用本地 IP 列表跑 CFST 不需要 curl 或 Xray：

```sh
./cftool.py --file ips.txt
```

需要复测时，在本机创建 `node.txt`，粘贴一条完整的 `vless://…` 分享链接（不要提交或分享），再运行：

```sh
chmod 600 node.txt
./cftool.py --file ips.txt --node node.txt --xray ../src-tauri/resources/bin/xray
```

复测需要 curl 和 Xray-core；若 `xray` 在 PATH 内可省略 `--xray`。快速复测可加 `--top 3 --rounds 2`。`--file` 每行可写 `IP:端口#备注` 或 HTTPS 明文订阅 URL（下载订阅需要 curl）。端口省略时取节点端口或 443；IPv6 带端口时写 `[IPv6]:端口`。订阅仅接受公网 IP，不递归订阅。按 IP + 端口去重。省略 `--file` 时使用 `../src/assets/ip.txt`，或用 `--ips` 指定 CFST 网段文件（如 `../src/assets/ipv6.txt`）。

macOS 上会在 CFST 启动前查询候选 IP 的系统路由（CIDR 每段抽查一个地址）。如果发现经过 TUN/VPN 接口，或路由无法确认，会打印提示但继续测速。检测不修改代理或路由设置，也不能判断隧道内是否实际转发给代理节点。

CFST 的原始结果从标准输出读取，不生成临时 CSV；CLI 仍将最终排名写入运行目录的 `result.csv`，覆盖上次结果。纯 CFST 模式记录无丢包候选的平均延迟；复测模式记录 HTTPS 成功次数和耗时中位数，全失败的入口保留但耗时留空。

## 测的是什么

- CFST 按候选端口分组做 TCP 延迟初筛，再按延迟合并各组，不测下载带宽。取无丢包候选中排名最前的 10 个。
- 每个候选启动一个仅监听本地回环的临时 Xray，替换连接 IP 和端口，不用该 IP 替换 TLS SNI 或 WS Host。仅支持 `type=ws`、`security=tls`、`encryption=none`、空 flow，以及 `sni`、`host`、`path`、`alpn`、`fp`、`ech`；`ech` 解码后交给支持 ECH 的 Xray。未知参数报错，不默默降级。
- 每个 IP 顺序发 5 次独立 curl 请求，经 SOCKS 代理访问 `https://www.gstatic.com/generate_204`，8 秒超时。只计完整返回 204 的请求；可用 `--url` 换成自己信任的 HTTPS 204 端点。不跟随重定向，不关闭证书校验。
- **成功次数优先，成功请求总耗时的中位数次之**；全失败不推荐。耗时含代理建连和目标响应，不是 ping。推荐只代表本轮候选、本地网络和测试目标；若开着 TUN/VPN，测到的可能是绕行后的路径。建议高峰期重测。

离线测试：`python3 -m unittest -v`。
