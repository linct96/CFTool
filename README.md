# cftool 桌面版

Tauri 2 + Rust，前端使用 React、TypeScript、Vite。界面参考 Cota：Tailwind CSS v4、shadcn/ui 的 Base UI / Mira / Neutral 预设、Inter，以及随系统切换的浅色和石墨深色主题。macOS 窗口采用自定义拖拽标题栏，保留原生红绿灯按钮，不显示应用名或版本号。

## 本地开发

当前目标为 macOS Apple Silicon（macOS 13.3 或更新版本）。开发需要 [Rustup 和 Xcode Command Line Tools](https://v2.tauri.app/start/prerequisites/#macos)、[fnm](https://github.com/Schniz/fnm) 和 pnpm。`.node-version` 指定 Node.js 26，`package.json` 指定 pnpm 版本，`rust-toolchain.toml` 锁定 Rust。运行桌面测速不需要 Python；后端由 Rust 调用应用内附带的 CFST、Xray，并在 Xray 复测时使用 macOS 的 `/usr/bin/curl`。

在项目根目录运行（先按 fnm 文档初始化当前 Shell）：

```sh
fnm install
fnm use
pnpm install --frozen-lockfile
pnpm dev
```

`pnpm dev` 启动 Tauri 桌面窗口，`pnpm dev:web` 只启动 Vite。开发模式可用 React Grab：指向界面元素，按住 `⌘C`（或 `Ctrl+C`）复制组件和源码位置；生产构建不加载它。不要分享含真实节点信息的选取结果。

- `pnpm lint`：Oxlint 类型感知检查，沿用 Cota 的 TanStack、React Hooks 和可访问性规则。
- `pnpm format` / `pnpm format:check`：用 Oxfmt 格式化 / 检查。
- `pnpm typecheck`：TypeScript 类型检查。
- `pnpm build`：先运行 lint 和类型检查，再构建前端。GitHub Actions 在 macOS ARM64 上检查前端格式和构建，以及 Rust 的 rustfmt 和 Clippy；暂不运行测试。

构建本机调试用的 `cftool.app`：

```sh
pnpm tauri build --debug --target aarch64-apple-darwin --bundles app -- --locked
```

产物：`src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/cftool.app`。未做分发签名或公证。

## 当前范围

- `src/App.tsx`：左侧导航切换「优选」和「设置」，切页不会清空输入或中断测速。
- `src/SettingsPage.tsx`：「设置」页左侧二级菜单切换「CFST 参数」和「依赖工具」，右侧展示对应详情。
- `src/ProbePage.tsx`：「优选」页填入 IP（每行一个，可带端口），即可在桌面应用中运行 CFST。VLESS 节点可选；留空不启动 Xray，填入则对每个节点追加最多 10 个候选 IP 的复测，支持带 `ech` 参数的 edgetunnel VLESS + WebSocket + TLS 分享链接。两个输入框明文显示。VLESS 节点编辑后会自动保存到 `~/Library/Application Support/com.cfspeed.desktop/nodes.txt`（仅当前用户可读写），重启时自动恢复，包括清空后的状态。
- 本次会话中完成并保存一轮有结果的纯 CFST 测速后，保持 IP 列表和 CFST 参数不变，开启「Xray 复测」即可「追加测速」。追加仅复用原结果的前 10 个入口，不重跑 CFST 或重新拉取订阅；Xray 的计划及结果写回同一条历史，保留原编号和 CFST 结果。追加暂停或中断后，可从该记录继续只测剩余线路。
- 桌面 Xray 复测只启动一个进程，按实际待测线路数 `N` 自动分组，并发数为 `min(4, max(1, floor(N / 3)))`（无候选时不启动）。各组在不同本地端口并行，组内按顺序轮转 3 轮，每轮每条线路做一次 HTTPS 204 探测；只有一条线路时，两次探测间至少等待 1 秒。不测下载速度。
- IP 列表的手动编辑和有效导入都会自动保存到 `~/Library/Application Support/com.cfspeed.desktop/ip-list.txt`（仅当前用户可读写）。重启时恢复完整文本，清空也会保存；只有尚无本地文件时才从随应用附带的 `src/assets/ip.txt` 和 `src/assets/ipv6.txt` 离线抽取 IPv4 / IPv6 候选。读取或保存失败会提示重试。
- IP 列表也可每行填写一个 HTTPS 订阅链接，与手填 IP 混用。仅在开始测速时拉取，原链接仍保存在本地，返回内容不会写回列表。订阅必须是 UTF-8 纯文本，每行一个公网 IPv4/IPv6 地址（可带端口和 `#` 备注）；不能嵌套订阅或使用 CIDR。链接不跟随重定向，应填写最终地址；最多同时填写 8 个不同链接，每个响应限 128 KiB / 1024 条。下载或格式错误会提示所在行并停止本轮测速。
- `src/ip-library.ts`：内置 [BestCF 优选 IP](https://bestcf.pages.dev/#yxip) 和 [edgetunnel](https://github.com/cmliu/edgetunnel) 使用的 [Cloudflare / 三网 CIDR 库](https://github.com/cmliu/cmliu)。可离线导入「Cloudflare IPv4」或「Cloudflare IPv6」，远程来源只在点击导入时拉取；纯文本与测速时展开订阅共用解析规则，导入会保存具体地址及 `#` 后的完整备注。不同备注可以对应同一个 IP 和端口，但测速只测一次。测速结果和历史回放在「IP / 端口」后按「机房/国家」显示 CFST 实测结果（如 `NRT/日本`），未知地区显示 `机房码/—`，未返回机房时显示 `—`。「备注」列直接显示 `#` 后的原备注。CIDR 每次导入最多抽取 128 个 IPv4 地址。
- `src/CfstSettings.tsx`：「设置」页的 CFST 参数用于下一次测速。桌面版会将有效修改自动保存到 `~/Library/Application Support/com.cfspeed.desktop/config.json` 的 `cfst` 字段，重启自动恢复。前端用 TanStack Form 和 Zod 校验并就地显示错误；启用 HTTPing 或下载测速时，自定义地址必须为不含账号密码的 HTTP / HTTPS URL，留空使用默认地址。没填默认端口时，纯 CFST 用 443；复测模式跟随节点端口。IP 行上显式填写的端口始终优先。
- `src/styles.css`：界面主题 token；`components.json` 固定 shadcn/ui 预设。
- `src-tauri/`：Rust 后端打包并调用 CFST 和 Xray，使用固定 IPC 启动、确认或取消测速，不开放通用 shell 权限。点击开始测速后，先校验候选 IP 和订阅链接，再检查 TUN/VPN 路由与代理设置。检测到风险或无法确认时，须确认后才创建任务、拉取订阅并启动 CFST。仅有订阅链接时，开测前通过本机路由表判断隧道风险，不提前下载订阅。候选列表校验完成后才写入可续测的历史。候选 IP 文件和 Xray 配置只在临时目录使用。
- 「依赖工具」首次打开只读取本地版本和 3 天内的检查缓存。点击「检查更新」会检查 CFST 和 Xray 的官方 GitHub 稳定版，并复用 60 秒内的成功结果；缓存保存在 `~/Library/Application Support/com.cfspeed.desktop/cache/`，可跨重启保留。有新版时点击版本按钮会重新核对 GitHub 发布信息并下载 macOS ARM64 发布包，校验 SHA-256、架构和版本后安装到 `~/Library/Application Support/com.cfspeed.desktop/engines/`，不改写 `.app`。下一轮测速自动使用比内置版更新的已下载版本；可点击「恢复内置版本」删除对应下载。测速过程中不能安装或恢复组件。
- 「测速历史 → 查看」在「优选」主界面回放原结果，可返回当前结果。每轮在 `~/Library/Application Support/com.cfspeed.desktop/history/` 中保存一个版本化 JSON，记录当时的生效参数、已展开的候选、任务状态和结果。CFST 原生 CSV 只作临时输入。已完成记录可单行重测，临时结果不会覆盖历史。旧 CSV 历史不会加载或迁移。
- 暂停或中断后，从历史选择「继续测速」，沿用原计划，只执行未保存为完成的任务。仅测延迟时首批大小取 CFST 并发数，后续按实测耗时向约 10 秒一批调整；启用下载时，同一分组和实际端口仍一次跑完。Xray 按节点与入口保存完整复测结果。历史只保存节点配置指纹，不保存 VLESS 链接或凭据；缺少原节点时，需先恢复本地节点配置。

改名后仍沿用原应用标识 `com.cfspeed.desktop`，已有节点和组件无需搬迁。

## 官方参考

- [Tauri 项目模板](https://v2.tauri.app/start/create-project/) · [Vite 集成](https://v2.tauri.app/start/frontend/vite/)
- [Tauri Capabilities](https://v2.tauri.app/security/capabilities/) · [CSP](https://v2.tauri.app/security/csp/) · [macOS 应用包](https://v2.tauri.app/distribute/macos-application-bundle/)
- [Tailwind CSS + Vite](https://tailwindcss.com/docs/installation/using-vite) · [shadcn/ui + Vite](https://ui.shadcn.com/docs/installation/vite)
