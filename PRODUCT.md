# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

macOS (Apple Silicon) power users who tune Cloudflare CDN edge IPs and optional VLESS + Xray proxy routes. Their primary workflow is iterative comparison and tuning: editing candidate IP lists and CFST parameters, running repeated probes, comparing latency/loss/speed across candidates, retesting individual rows, and occasionally checking archived probe history.

## Product Purpose

CFTool is a local desktop utility for screening Cloudflare candidate IPs with CFST and optionally verifying the top candidates end-to-end through user-provided VLESS nodes via Xray. Success means rapidly adjusting inputs and parameters, seeing active probe configuration and progress clearly, and confidently picking the best IP/node combinations inside a compact desktop window.

## Positioning

Unlike CLI-only scripts or bloated proxy clients, CFTool pairs local CFST scanning and multi-round Xray HTTPS 204 retesting in one focused, private macOS desktop tool—preserving local inputs, auto-saving validated parameters, showing original IP remarks inline, and letting users retest single rows without polluting saved history.

## Operating Context

- Runs as a Tauri 2 macOS desktop app (`920×600` default window, `640×440` minimum) with macOS overlay titlebar and native traffic light controls.
- Tech stack stays consistent: Tauri 2 + Rust, React 19, TypeScript, Vite, Tailwind CSS v4, shadcn/ui (Base UI), TanStack Form + Zod.
- Outer window chrome & sidebar layout are preserved: 44px drag header + 48px left icon navigation rail (`优选` at top, `设置` at bottom) + rounded top-left `<main>` container (`rounded-tl-2xl border-t border-l`).
- Two-module top-level structure is preserved: **优选 (Probe)** and **设置 (Settings: CFST 参数 + 依赖工具)**, keeping drafts and in-flight probes or engine updates mounted across page switches.
- Local-first persistence: `ip-list.txt`, `nodes.txt`, `config.json`, `history/<utc>/result.csv`, and downloaded `cfst`/`xray` binaries in user app data without mutating the `.app` bundle.

## Brand Commitments

- **Preserved outer frame**: Keep the existing `App.tsx` outer shell and slim left icon sidebar layout.
- **Preserved color system**: Keep the existing shadcn/ui Mira / Neutral (light) and Graphite Studio (dark) color tokens in `src/styles.css`, keeping automatic light/dark polarity.
- **Refactor focus**: Completely redesign the `<main>` content area's internal visual hierarchy, spatial layout, and interaction ergonomics for both `优选` and `设置`.

## Capabilities and Constraints

- Must preserve the two-page top-level architecture (`优选` and `设置`), while completely redesigning internal page layout, information hierarchy, and visual system.
- Must work comfortably at `920×600` default and down to `640×440` minimum without cramped split panes or awkward clipping, while respecting the macOS overlay traffic lights (`x: 16, y: 20`).
- Must support both light and dark modes automatically following system appearance (`prefers-color-scheme`).
- Copy discipline: zero filler, zero permanent business-rule prose; every visible label, badge, or feedback line must directly assist the current task, data reading, or next action.
- Core flows to retain: IP list editing & HTTPS subscriptions, 8 built-in IP libraries (BestCF & edgetunnel), optional VLESS node list, TUN/VPN pre-probe route confirmation dialog, streaming progress & cancellation, unified CFST + Xray comparison table with original IP remarks, per-row temporary retest (`重测 IP` / `测·重测 Xray`), UTC probe history viewing & deletion, 14-field validated CFST parameter form with mode-dependent disabling, and CFST/Xray engine update check/install/restore.

## Product Principles

- **Comparison-first density**: In a compact desktop window, candidate metrics and active probe state should command the primary visual real estate, while input editors and secondary panels stay accessible without permanently choking the results table.
- **At-a-glance configuration awareness**: Key active probe parameters (mode, port, download test, colo filter, VLESS mode) should be immediately legible from the Probe workspace so iterative tuning does not require blind tab-switching.
- **Zero-fluff operational clarity**: Enforce constraints through disabled states, inline validation, pre-run confirmation, and concise status output rather than explanatory paragraphs.
- **System-native dual polarity**: Both light and dark themes must feel intentionally crafted—never an inverted afterthought—fitting naturally into macOS desktop ergonomics.
