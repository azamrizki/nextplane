# NextPlane design

Since 2026-11-21 NextPlane deliberately looks and behaves like Plane's web app, so people who know Plane can work without relearning (owner decision, SPEC §2). The design is rebuilt from a written description of Plane's patterns and values, in our own code: no Plane component, CSS, icon artwork, illustration, wording, logo or name.

## Principles

1. **Familiar first.** Layout, density, colors and shortcuts follow Plane's current web app, so someone coming from Plane finds everything where they expect it.
2. **Dense but calm.** 13 px body text (Inter), 44 px rows, hairline borders, color only where it carries meaning (state, priority, the one primary action).
3. **Keyboard first.** Every frequent action has a shortcut (Plane's set) and is reachable from the command palette (`Ctrl/⌘K`).
4. **Two themes, one system.** Light and dark use the same tokens; components never hard-code colors.

## Tokens

All tokens live in `web/src/styles/tokens.css` as CSS variables. Values follow Plane's current palette (read from its published design tokens, then written here as our own variables).

| Group | Tokens | Notes |
|---|---|---|
| Type | `--font-sans` (Inter Variable), `--font-mono` (IBM Plex Mono), `--text-xs` 11 · `--text-sm` 12 · `--text-base` 13 · `--text-md` 14 · `--text-lg` 16 · `--text-xl` 20 · `--text-2xl` 24, weights 450/500/600 | Fonts are bundled (`@fontsource`, OFL). |
| Space | `--space-1` (4 px) … `--space-7` (48 px), `--page-x` 22 px | Page gutter as in Plane. |
| Radius | `--radius-sm` 4, `--radius-md` 6, `--radius-lg` 8, `--radius-xl` 12 | Controls use 6. |
| Controls | `--ctl-xs` 20 · `--ctl-sm` 24 (chips) · `--ctl-md` 28 (buttons) · `--ctl-lg` 32 (inputs) · `--ctl-xl` 36 | |
| Surfaces | `--canvas` (behind the panel), `--bg` (surface), `--bg-subtle`, `--layer-1/2/3` (+ `-hover`, `-active`), `--bg-hover` / `--bg-pressed` / `--bg-active` (transparent rows) | Light canvas `#f2f2f3`, dark `#0f0f10`. |
| Lines | `--border` (subtle), `--border-1` (cards, chips), `--border-strong`, `--border-strong-1` | |
| Text | `--text`, `--text-secondary`, `--text-muted`, `--text-faint`, `--icon` | Four steps. |
| Accent | `--accent` (`#006399` light / `#2893cc` dark), `--accent-hover`, `--accent-active`, `--accent-fg`, `--accent-soft(-hover)` | The one brand color. |
| Status | `--danger(-fg, -soft)`, `--warning-fg/-soft`, `--success(-soft)`, `--focus` | |
| Priority | `--prio-urgent/high/medium/low/none`, `--prio-unlit` | Per theme. |
| State group | `--group-backlog/unstarted/started/completed/cancelled` | Defaults for new states. |
| Shadow | `--shadow-sm`, `--shadow-raised`, `--shadow-md` (menus), `--shadow-lg` (dialogs), `--shadow-side` (peek) | |
| Layout | `--sidebar-w` 250 px, `--topbar-h` 44 px, `--header-h` 48 px | |

## App frame

- **Top bar** on the canvas: workspace switcher (left), search that opens the command palette (center), inbox with an unread dot, help menu and user menu (right).
- **Panel** below it, 8 px from the window edges, rounded and bordered, holding the sidebar and the page.
- **Sidebar** (250 px, `Ctrl/⌘B` hides it; an overlay below 768 px): title and collapse button, a full-width "New work item" button (opens the create modal with a project picker), Home / Your work / Drafts, a collapsible **Workspace** section (Projects, Analytics), and **Projects**: each joined project expands to Work items, Cycles, Modules, Pages, Intake (with its open count), with a ⋯ menu (Publish, Copy link, Settings, Leave). The footer shows the version.
- **Page header** (48 px, `PageHeader`): breadcrumbs on the left, the last one being the page title (`h1`), actions on the right. Project pages use `ProjectBar`: the project crumb (its chevron opens a project switcher), the section, deeper crumbs, then the actions.
- **Settings** pages put a menu on the left (`SettingsLayout`) and the form on the right.

## Work items

- **Header actions:** saved views, search, layout switcher (List, Board, Calendar, Spreadsheet, Timeline), Filters (funnel; accent while filters apply), Display, Analytics, primary "Add work item".
- **Filter row:** applied filters as joined pills `[icon Property][values][×]`, "Add filter", "Clear filters".
- **Display:** property toggles as pills, group by, order by, show sub-items, show empty groups.
- **List:** 44 px group headers (icon, name, count, "+") that collapse on click; 44 px rows (hover checkbox in the gutter, ID at one width so titles align, title, chips on the right, ⋯ menu); "New work item" quick add at the end of each group.
- **Board:** 336 px columns on `--layer-1`, collapsible to a 44 px strip; cards with ID, wrapping title and wrapping chips.
- **Chips** (`WorkItemProps`): `chip` = 24 px outlined pill, an empty property shrinks to an icon square; `pill` = the same with a placeholder name (create modal); `field` = borderless full-width control (detail view, spreadsheet). Order: State, Priority, Start, Due, Assignees (≤3 stacked faces), Modules, Cycle, Estimate, sub-item count, Labels.
- **Peek:** a side panel over the content (half width), with close, open full page, subscribe, copy link and ⋯. **Detail page:** title, the five main properties as a row of fields, description, sub-items, relations, attachments, links, activity and comments; a 320 px properties panel (labels, estimate, parent, cycle, modules, then who and when).

## Icons

`lucide-react` (ISC) at 16 px, stroke 1.5, mapped to Plane's meanings (e.g. Work items `ListTodo`, Cycles `Contrast`, Modules `Boxes`, Pages `FileText`, Intake `Import`). State and priority glyphs are our own SVGs drawn to Plane's semantics: backlog = dotted ring, unstarted = ring, started = ring with an inner ring, completed = disc with a check cut out, cancelled = disc with a cross cut out; urgent = outlined square with "!", high/medium/low = three signal bars with 3/2/1 lit.

## Keyboard

`Ctrl/⌘K` palette, `Ctrl/⌘B` sidebar, `Ctrl/⌘/` or `?` shortcut list, `C` new work item in a project, `N` then `I` new work item anywhere, `N` then `P` new project, `G` then `H`/`X`/`Y`/`P`/`A`/`J`/`S` to Home, Inbox, Your work, Projects, Analytics, Drafts, Settings, and inside a project `G` then `I`/`C`/`M`/`D`/`K` to Work items, Cycles, Modules, Pages, Intake (`web/src/lib/shortcuts.ts`).

## Components

`.btn` (secondary look; `-primary`, `-danger`, `-danger-outline`, `-ghost`, `-sm` 24, `-lg` 32, `-block`), `.icon-btn` (+ `-sm`, `-xs`, `.bordered`), `.seg` (segmented control), `.menu` / `.menu-item` (32 px rows), `Picker` (searchable combobox with checks), `Dialog` (header, body, footer with Cancel then primary; `bare` for the palette), `.badge`, `.card`, `.switch`, `.keycap`. React wrappers in `web/src/components/ui.tsx`.

**Layers.** Every menu, picker and small popup (`Popover`, `Picker`, the mention menu) floats in a layer of its own (`Floating.tsx`): rendered outside the page (into `body`, or into the open modal dialog it belongs to), in fixed position next to its trigger, flipped above when there is more room there, kept inside the window, and only as tall as the room it has (the menu scrolls). No board column, list row, chip row, panel, dialog body or sidebar can cut one off. Toasts are a manual popover in the browser's top layer, so they show above modal dialogs. Do not position a menu with `position: absolute` inside a scrolling container. Crowded page headers (four or more actions) turn secondary buttons into icons below 860 px (they keep their names as tooltips), and breadcrumbs end in "…" instead of disappearing.

**UI audit.** `NEXTPLANE_AUDIT=<folder> npx playwright test -c e2e/playwright.config.ts audit` opens every menu and picker where it lives, at 1280×800 and 1024×600, and checks with `elementFromPoint` that each is fully on screen and not covered; it also flags pages wider than the window. `NEXTPLANE_AUDIT_ALL=1` keeps a screenshot of every case.

## Screenshots

`NEXTPLANE_SHOTS=<folder> npx playwright test -c e2e/playwright.config.ts shots` seeds a throwaway server and photographs the main screens in light and dark for review.

## Charts

Charts follow one method (the dataviz rules the project adopted in S2.2 and S3.4):

- **Form first.** A single number is a stat tile, not a chart; parts of a whole are horizontal bars (ranked, or in their natural order: state groups, priorities); change over time is a line; a second dimension is a stacked bar.
- **Color by job.** One-series bars use one hue (`--series-1`). Series and segments take `--series-1` … `--series-8` in that fixed order (ordered values such as priorities keep a fixed slot so a value never changes color); a folded tail is "Other" in `--series-other`. Never more hues: past seven segments the server folds the rest into "Other". Status colors are not used for series.
- **Validated palette.** The eight series colors are the dataviz reference palette, checked with its validator against our surfaces (light `#ffffff`, dark `#141415`): all checks pass. In light mode slots 3–5 are below 3:1 contrast, so every chart has a legend (two or more series), a tooltip on hover and keyboard focus, and a table view with the same numbers.
- **Marks.** Bars at most 18px thick, 4px rounded end, square at the baseline, value at the tip; 2px gaps between stacked segments; 2px lines with 8px end dots ringed in the surface color; hairline grid; whole-number ticks for counts; text in text tokens, never in the series color. SVG text is drawn at its real pixel size (the chart measures its width).
- **One axis**, always. Filters sit in one row above the charts they scope; while new numbers load, the old chart stays at reduced opacity.

## Language

English source strings in `web/src/i18n/en.ts`; Indonesian in `id.ts` with the same keys and placeholders (a test checks both). The choice is stored in `localStorage["np.lang"]`, defaulting to the browser language.
