---
name: CFTool
description: macOS Cloudflare edge IP & VLESS probe desktop utility built on shadcn/ui Mira Neutral & Graphite Studio tokens
colors:
  background-light: 'oklch(1 0 0)'
  background-dark: 'oklch(0.252 0 0)'
  sidebar-light: 'oklch(0.985 0 0)'
  sidebar-dark: 'oklch(0.236 0 0)'
  muted-light: 'oklch(0.97 0 0)'
  muted-dark: 'oklch(0.215 0 0)'
  foreground-light: 'oklch(0.145 0 0)'
  foreground-dark: 'oklch(0.935 0 0)'
  primary-light: 'oklch(0.205 0 0)'
  primary-dark: 'oklch(0.922 0 0)'
  border-light: 'oklch(0.922 0 0)'
  border-dark: 'oklch(1 0 0 / 11%)'
  status-working: '#38bdf8'
  status-ok-light: 'oklch(0.508 0.148 152)'
  status-ok-dark: 'oklch(0.765 0.145 160)'
  status-warn-light: 'oklch(0.58 0.155 62)'
  status-warn-dark: 'oklch(0.8 0.145 78)'
  status-temp-light: 'oklch(0.49 0.135 240)'
  status-temp-dark: 'oklch(0.78 0.12 235)'
  destructive-light: 'oklch(0.577 0.245 27.325)'
  destructive-dark: 'oklch(0.704 0.191 22.216)'
typography:
  title:
    fontFamily: "'Inter Variable', system-ui, sans-serif"
    fontSize: '0.875rem'
    fontWeight: 600
    lineHeight: 1.4
    letterSpacing: '-0.015em'
  body:
    fontFamily: "'Inter Variable', system-ui, sans-serif"
    fontSize: '0.75rem'
    fontWeight: 400
    lineHeight: 1.625
  mono:
    fontFamily: "ui-monospace, 'SF Mono', Menlo, monospace"
    fontSize: '0.75rem'
    fontWeight: 400
    lineHeight: 1.5
  caption:
    fontFamily: "ui-monospace, 'SF Mono', Menlo, monospace"
    fontSize: '0.6875rem'
    fontWeight: 500
    lineHeight: 1.4
rounded:
  sm: '0.375rem'
  md: '0.5rem'
  lg: '0.625rem'
  2xl: '1.125rem'
spacing:
  xs: '4px'
  sm: '8px'
  md: '12px'
  lg: '16px'
components:
  button-primary:
    backgroundColor: '{colors.primary-light}'
    textColor: '{colors.sidebar-light}'
    rounded: '{rounded.md}'
    height: '28px'
    padding: '0 8px'
  button-outline:
    backgroundColor: '{colors.background-light}'
    textColor: '{colors.foreground-light}'
    rounded: '{rounded.md}'
    height: '28px'
    padding: '0 8px'
  config-pill:
    backgroundColor: '{colors.muted-light}'
    textColor: '{colors.foreground-light}'
    rounded: '{rounded.sm}'
    height: '24px'
    padding: '0 8px'
---

# Design System: CFTool

## Overview

**Creative North Star: "Quiet Precision Workbench"**

CFTool is a compact macOS desktop utility built on shadcn/ui's Mira Neutral (light) and Graphite Studio (dark) OKLCH surfaces. Visual noise stays near zero so Cloudflare IP latency, packet loss, IATA colo codes, and Xray HTTPS 204 ranges carry the screen without competing chrome.

The outer shell keeps a 44px macOS overlay drag bar and a 48px left icon rail framing a rounded top-left workspace canvas. Inside that canvas, results take the full width while candidate editing and archived history slide in on demand from right-hand sheets.

**Key Characteristics:**

- Dual-polarity OKLCH neutral tokens (`src/styles.css`) adapting automatically to `prefers-color-scheme`.
- Full-width results canvas with right-hand slide-over sheets for secondary authoring and history lookup.
- Tabular monospace alignment for all endpoints, latencies, packet loss, and CLI flags.
- Zero-fluff operational copy enforced through states, validation, and concise status readouts.

## Colors

A restrained monochromatic neutral ramp in OKLCH (pure white to warm-free ink in light mode; layered graphite in dark mode) paired with tightly scoped semantic colors used strictly for network health, running state, temporary retest highlights, and errors.

### Primary

- **Ink Primary** (`--primary`): High-contrast dark ink in light mode and crisp light ink in dark mode; reserved for the single primary run action, active top-ranked IATA colo badge, and checked switches.

### Neutral

- **App Chrome Muted** (`--muted`): Outer macOS window frame, segmented tab tracks, and subtle pill backgrounds.
- **Subtle Surface** (`--sidebar`): Sticky table headers, summary strips, and footer status bars.
- **Workspace Canvas** (`--background`): Primary content surface inside the rounded top-left `<main>` frame and slide-over sheets.
- **Hairline Border** (`--border`): 1px structural dividers separating toolbars, table rows, cards, and sheets.

### Semantic Status

- **Working Sky** (`--dot-status-working`): Active probe pulse dot and slim 2px indeterminate progress bar.
- **Clear Emerald** (`--status-ok` / `--status-ok-surface`): Zero packet loss and fastest candidate latency.
- **Degraded Amber** (`--status-warn` / `--status-warn-surface`): Non-zero packet loss or degraded candidates.
- **Temporary Retest Tint** (`--status-temp` / `--status-temp-surface`): Highlights rows updated by single-row temporary retesting so users immediately distinguish live overrides from archived results.
- **Destructive Red** (`--destructive`): Validation errors, storage failures, and destructive delete/cancel actions.

**The Semantic Restraint Rule.** Chromatic hues appear only on real operational states (running, 0% loss/fastest, degraded loss, temporary retest, or error) — never on static chrome or decoration.

## Typography

**Body Font:** `Inter Variable`, `system-ui`, `sans-serif`
**Data / Flag Font:** `ui-monospace`, `SF Mono`, `Menlo`, `monospace` (always with `tabular-nums`)

**Character:** Compact desktop UI scale (`12px` base, `14px` section titles) tuned for high information density at `920×600` and `640×440`.

### Hierarchy

- **Page Title** (600, `0.875rem`, `-0.015em` tracking): Single-line toolbar heading (`IP 优选`, `设置`).
- **Group Heading** (600, `0.75rem`): Card fieldset legends and engine section headings.
- **Body / Controls** (400–500, `0.75rem`, `1.625` line-height): Buttons, tabs, labels, table cells.
- **Data / Flags / Badges** (400–600, `0.6875rem`–`0.75rem`, monospace `tabular-nums`): IP endpoints, latency numbers, CLI flags (`-n`, `-tp`, `-cfcolo`), and IATA colo badges.

## Layout

- **Outer Frame (preserved):** `44px` top drag bar (traffic lights at `x: 16, y: 20`, right running indicator) + `48px` left icon rail (`优选` top, `设置` bottom) + `rounded-tl-2xl border-t border-l` workspace `<main>`.
- **Probe Workspace (`优选`):** Single-row toolbar → conditional `2px` running progress bar → compact summary strip + full-width sticky-header table (or empty state with quick `编辑候选源` action) → `28px` footer status bar.
- **Slide-Over Sheets:** Right-anchored Base UI `Sheet` (`max-w-[min(26rem,90vw)]` for candidate editor with `IP 列表` / `VLESS 节点` tabs; `max-w-[min(28rem,92vw)]` expanding to `max-w-[min(56rem,94vw)]` for history detail).
- **Settings Workspace (`设置`):** Single-row toolbar with `Tabs` (`CFST 参数` / `依赖工具`) — no redundant nested sidebar — over a centered `@container`-responsive 2–3 column card grid (`max-w-3xl`).

## Elevation & Depth

Flat hairline-divided surfaces by default; depth is reserved strictly for top-layer overlays (`Dialog`, `AlertDialog`, `Sheet`) using `--overlay` backdrop scrims and `--shadow-sheet`.

**The Flat-Workspace Rule.** Content panels, summary strips, and form cards rely on `1px` `--border` hairlines and `--sidebar` tonal contrast rather than drop shadows.

## Shapes

Consistent Mira radius scale anchored at `--radius: 0.625rem`:

- Small badges, chips, and segmented tabs: `--radius-sm` (`6px`).
- Buttons, inputs, and list items: `--radius-md` (`8px`).
- Form group cards and modals: `--radius-lg` (`10px`).
- Workspace top-left cutout: `--radius-2xl` (`18px`).

## Components

### Buttons

- Compact `28px` default height (`24px` sm, `20px` xs) via shadcn/Base UI `Button`.
- Primary (`default`), secondary border (`outline`), quiet (`ghost`), and low-glare tinted `destructive` variants.

### Segmented Tabs (`Tabs`)

- Built on `@base-ui/react/tabs`; muted bordered pill track (`bg-muted border-border`) with elevated `bg-background` active tab. Shared consistently between the Candidate Drawer and Settings header.

### Slide-Over Sheets (`Sheet`)

- Built on `@base-ui/react/dialog`; full-height right-aligned drawer with fixed `44px` header and Esc/backdrop dismiss. Keeps candidate drafts mounted (`keepMounted`) across open/close toggles.

### Comparison Table & Xray Whisker (`ProbeResults`)

- Semantic `<table>` with `border-separate border-spacing-0`, sticky `thead`, keyboard-scrollable `tabIndex={0} role="region"` wrapper, click-to-copy IP endpoint buttons, a separate measured colo/country column, inline original IP remarks, and inline min–median–max range whisker bars.

## Do's and Don'ts

### Do:

- **Do** manage all colors, radii, and shadows through CSS tokens in `src/styles.css`.
- **Do** keep tabular numbers (`tabular-nums font-mono`) on every IP, latency, packet-loss, and count readout.
- **Do** keep both `ProbePage` and `SettingsPage` (and both settings tabs) mounted so drafts and in-flight probes or engine updates survive navigation.

### Don't:

- **Don't** add explanatory business-rule paragraphs or duplicate headings to pages, drawers, or forms.
- **Don't** split the `920×600` Probe canvas permanently with a fixed input sidebar that chokes the comparison table.
- **Don't** introduce off-system saturated brand accents outside the semantic status tokens.
