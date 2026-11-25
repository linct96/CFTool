---
version: 1
slug: 'src-probepage-tsx'
primary_target: 'src/ProbePage.tsx'
related_targets:
  [
    'src/ProbeResults.tsx',
    'src/ProbeHistory.tsx',
    'src/SettingsPage.tsx',
    'src/CfstSettings.tsx',
    'src/EngineUpdates.tsx',
    'src/App.tsx',
    'src/styles.css',
  ]
---

# Main Workspace Surface Brief

- **Scope:** `src/ProbePage.tsx`, `src/ProbeResults.tsx`, `src/ProbeHistory.tsx`, `src/SettingsPage.tsx`, `src/CfstSettings.tsx`, `src/EngineUpdates.tsx`, `src/App.tsx`, `src/styles.css`
- **Mode:** Operate
- **Audience & Job:** macOS Apple Silicon users iteratively tuning Cloudflare edge IPs and optional VLESS + Xray routes inside a `920×600` (min `640×440`) Tauri desktop window.
- **Constraints:** Preserve the existing `App.tsx` outer shell (44px overlay drag header + 48px left icon rail + `rounded-tl-2xl` `<main>`) and existing shadcn/ui Mira Neutral (light) / Graphite Studio (dark) OKLCH color system; keep zero-fluff copy; keep TanStack Form + Zod and all 15 Rust IPC commands intact.

## Direction contract

- **THESIS:** Give the full `<main>` canvas to live candidate comparison and active configuration awareness, refusing the cramped permanent split-pane and nested-modal history layout.
- **OWN-WORLD:** Exact shadcn Mira / Neutral (light) and Graphite Studio (dark) OKLCH surfaces with 1px hairline borders, tabular monospace metrics, restrained semantic status tokens (`ok`, `warn`, `temp`, `working`), and crisp Base UI slide-over sheets and segmented controls.
- **STORY:** Users see active CFST flags and candidate counts immediately on arrival, open a focused right-hand tabbed drawer only when editing IPs or VLESS links, run probes with clear inline progress, compare CFST + Xray latency ranges at full table width, and inspect/retest/copy winners or review history without leaving context.
- **FIRST VIEWPORT:** Inside the preserved left-rail shell, `<main>` opens with a single-row toolbar (`IP 优选`, clickable active CFST parameter chip + VLESS count chip, `候选源` drawer trigger, `历史` drawer trigger, primary `开始测速` button), an optional thin progress bar while running, a compact 3-metric fastest-route summary strip when results exist, a full-width sticky-header comparison table with IATA colo badges and inline Xray min–median–max range whiskers plus quick IP copy and per-row retest buttons, and a slim footer status bar. On `设置`, remove the redundant nested sidebar in favor of a top segmented tab bar (`CFST 参数` / `依赖工具`) over a 3-column responsive grouped form.
- **FORM:** Scheme A — Results-First Canvas + Right Slide-Over Tabbed Drawers (Candidate #6, dealt lead; `seed:e09215fb53160069`).
- **FINISH:** unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
