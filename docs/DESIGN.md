# NextPlane design

NextPlane's look is its own. Plane is a reference for **what** the product does, never for how it looks: no Plane colors, icons, illustrations, layouts copied pixel for pixel, or wording (SPEC §2).

## Principles

1. **Dense but calm.** Project tools are read all day: small type (14 px body, 12.5 px secondary), tight rows, few borders, color only where it carries meaning (priority, state group, accent for the one main action).
2. **Keyboard first.** Every frequent action has a shortcut and is reachable from the command palette (`Ctrl/⌘K`).
3. **Instant.** Optimistic updates for edits and drags; spinners only for first loads.
4. **Two themes, one system.** Light and dark use the same tokens; components never hard-code colors.

## Tokens

All tokens live in `web/src/styles/tokens.css` as CSS variables.

| Group | Tokens | Notes |
|---|---|---|
| Type | `--font-sans`, `--font-mono`, `--text-xs` … `--text-2xl` | System font stack, nothing downloaded. |
| Space | `--space-1` (4 px) … `--space-7` (48 px) | 4 px grid. |
| Radius | `--radius-sm` 4, `--radius-md` 6, `--radius-lg` 10, `--radius-full` | |
| Surface | `--bg`, `--bg-subtle`, `--bg-muted`, `--bg-hover`, `--bg-active`, `--bg-overlay` | |
| Line | `--border`, `--border-strong` | |
| Text | `--text`, `--text-muted`, `--text-faint` | |
| Accent | `--accent` (teal `#0e7c6b` light / `#23a68f` dark), `--accent-hover`, `--accent-text`, `--accent-soft` | The one brand color. |
| Status | `--danger`, `--danger-soft`, `--warning-soft`, `--success`, `--focus` | |
| Priority | `--prio-urgent`, `--prio-high`, `--prio-medium`, `--prio-low`, `--prio-none` | Same in both themes. |
| State group | `--group-backlog`, `--group-unstarted`, `--group-started`, `--group-completed`, `--group-cancelled` | Defaults for new states. |
| Motion | `--dur-fast` 90 ms, `--dur-med` 160 ms, `--ease` | |
| Layout | `--sidebar-w` 248 px, `--header-h` 48 px | |

## Theme

`data-theme="light" | "dark"` on `<html>`, set before first paint from `localStorage["np.theme"]` (`system`, `light`, `dark`). `system` follows `prefers-color-scheme` live.

## Components (so far)

`.btn` (+ `-primary`, `-danger`, `-ghost`, `-sm`, `-block`), `.icon-btn`, `.field` / `.input` / `.select` / `.textarea`, `.alert-error`, `.badge`, `.auth-card`, `.spinner`. React wrappers in `web/src/components/ui.tsx`.

## Charts

Charts follow one method (the dataviz rules the project adopted in S2.2 and S3.4):

- **Form first.** A single number is a stat tile, not a chart; parts of a whole are horizontal bars (ranked, or in their natural order: state groups, priorities); change over time is a line; a second dimension is a stacked bar.
- **Color by job.** One-series bars use one hue (`--series-1`). Series and segments take `--series-1` … `--series-8` in that fixed order (ordered values such as priorities keep a fixed slot so a value never changes color); a folded tail is "Other" in `--series-other`. Never more hues: past seven segments the server folds the rest into "Other". Status colors are not used for series.
- **Validated palette.** The eight series colors are the dataviz reference palette, checked with its validator against our surfaces (light `#ffffff`, dark `#121417`): all checks pass. In light mode slots 3–5 are below 3:1 contrast, so every chart has a legend (two or more series), a tooltip on hover and keyboard focus, and a table view with the same numbers.
- **Marks.** Bars at most 18px thick, 4px rounded end, square at the baseline, value at the tip; 2px gaps between stacked segments; 2px lines with 8px end dots ringed in the surface color; hairline grid; whole-number ticks for counts; text in text tokens, never in the series color. SVG text is drawn at its real pixel size (the chart measures its width).
- **One axis**, always. Filters sit in one row above the charts they scope; while new numbers load, the old chart stays at reduced opacity.

## Language

English source strings in `web/src/i18n/en.ts`; Indonesian in `id.ts` with the same keys and placeholders (a test checks both). The choice is stored in `localStorage["np.lang"]`, defaulting to the browser language.
