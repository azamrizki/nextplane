# NextPlane design

NextPlane works like Plane's web app: the same layout, density, interaction patterns and shortcuts, so people who know Plane can work without relearning (owner decision 2026-11-21, SPEC §2). Since v0.3 the surfaces are our own: **NextPlane Glass** (owner decision 2026-11-22). It is flat, quiet frosted glass over a near-neutral scene, with our own form controls throughout.

The design is rebuilt in our own code from a written description of Plane's patterns: no Plane component, CSS, icon artwork, illustration, wording, logo or name. The mockups and the review behind Glass are kept outside the repository, in `.generate/overhoul-ui-glass`.

## Principles

1. **Familiar first.** Layout, density and shortcuts follow Plane's current web app, so someone coming from Plane finds everything where they expect it.
2. **Flat glass, no effects.** Depth comes from layers of frosted glass, not from lighting:
   - every surface is a tint, a blur, an even hairline edge and a soft shadow;
   - no gradients, bevels, glow or text shadows, on buttons or anywhere else;
   - the owner rejected all of those as generic "AI" styling.
3. **Dense but calm.** 13 px body text (Inter), 44 px rows, hairline borders, color only where it carries meaning (state, priority, the one primary action).
4. **Our own controls.** No browser-drawn select, date, colour or number control; checkboxes and radios keep the native input under our look, so labels, forms and screen readers keep working.
5. **Keyboard first.** Every frequent action has a shortcut (Plane's set) and is reachable from the command palette (`Ctrl/⌘K`).
6. **Two themes, one system.** Light and dark use the same tokens; components never hard-code colors.

## Tokens

All tokens live in `web/src/styles/tokens.css` as CSS variables. Names stay from the Plane-like v0.1 set; values are NextPlane Glass.

| Group | Tokens | Notes |
|---|---|---|
| Type | `--font-sans` (Inter Variable), `--font-mono` (IBM Plex Mono), `--text-xs` 11 · `--text-sm` 12 · `--text-base` 13 · `--text-md` 14 · `--text-lg` 16 · `--text-xl` 20 · `--text-2xl` 24, weights 450/500/600 | Fonts are bundled (`@fontsource`, OFL). |
| Space | `--space-1` (4 px) … `--space-7` (48 px), `--page-x` 22 px | Page gutter as in Plane. |
| Radius | `--radius-sm` 6 (chips), `--radius-md` 8 (buttons, inputs, menu items), `--radius-lg` 12 (cards, menus), `--radius-xl` 16 (dialogs, board columns), `--radius-2xl` 20 (the app panel) | One step rounder than v0.2. |
| Controls | `--ctl-xs` 20 · `--ctl-sm` 24 (chips) · `--ctl-md` 28 (buttons) · `--ctl-lg` 32 (inputs) · `--ctl-xl` 36 | |
| Scene | `--canvas` with `--scene-1` / `--scene-2` (two faint radial tints) | The near-neutral backdrop behind the panel: light `#eef0f3`, dark `#0c0e11`. |
| Surfaces | `--bg` (the page, opaque: the glass panel's look), `--bg-subtle`, `--layer-1/2/3` (+ `-hover`, `-active`), `--bg-hover` / `--bg-pressed` / `--bg-active` (translucent rows) | |
| Glass | `--glass-float` (menus, pickers, dialogs, toasts, the peek), `--glass-blur` 28 px, `--glass-saturate` 140 %, `--glass-edge` (an even inner hairline) | Only floating layers blur. |
| Controls | `--btn-bg` / `-hover` / `--btn-border` / `--btn-shadow` (flat glass buttons), `--well` / `--well-hover` (inputs) | |
| Lines | `--border` (subtle), `--border-1` (cards, chips), `--border-strong`, `--border-strong-1` | |
| Text | `--text`, `--text-secondary`, `--text-muted`, `--text-faint`, `--icon` | Four steps. |
| Accent | `--accent` (`#1a6dc4` light / `#2370cc` dark), `--accent-hover`, `--accent-active`, `--accent-fg`, `--accent-soft(-hover)`, `--accent-border-soft` | The one brand color, used flat: a solid fill only on the primary action and checked controls, a soft tint for selection, a ring for focus. White text on either fill passes 4.5:1. The logo uses it too. |
| Status | `--danger(-fg, -soft, -border)`, `--warning-fg/-soft`, `--success(-soft)`, `--focus` | Danger `#b42318`. |
| Priority | `--prio-urgent/high/medium/low/none`, `--prio-unlit` | Per theme. |
| State group | `--group-backlog/unstarted/started/completed/cancelled` | Defaults for new states. |
| Shadow | `--shadow-sm`, `--shadow-raised`, `--shadow-md` (menus), `--shadow-lg` (dialogs), `--shadow-side` (peek), `--shadow-panel` | Soft; no glow. |

**Glass rules.**

1. **Where the blur goes.** Only floating layers blur: menus, pickers, the peek, dialogs and toasts. The page and its cards are opaque surfaces, so long lists stay cheap to scroll.
2. **A dialog carries its blur on `::before`.** An element with its own `backdrop-filter` becomes a "backdrop root", and any glass inside it would only be see-through, not blurred. Menus open inside dialogs, so the dialog's blur sits on `::before` and its body scrolls.
3. **Fallbacks.**
   - `prefers-reduced-transparency` swaps the glass for solid surfaces.
   - `forced-colors` gets the browser's own colours.
   - `prefers-reduced-motion` stops the motion.
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

**Buttons.**

- `.btn` is flat glass: a translucent fill and a hairline.
- Variants:
  - `-primary`: solid accent;
  - `-danger-outline`: red text, for everyday destructive actions;
  - `-danger-ghost`;
  - `-danger`: solid red, only for the last step of a confirmation;
  - `-ghost`, `-accent`;
  - sizes `-sm` 24 and `-lg` 32, plus `-block`.
- Icons in buttons take the label's colour at 70 % (100 % on hover) and are 14 px, so icon and text read as one.
- Icon buttons: `.icon-btn` (+ `-sm`, `-xs`, `.bordered`).

**Other parts.**

- `.seg` (segmented control: a sunken track with a raised thumb).
- `.menu` / `.menu-item` (frosted, 32 px rows).
- `Picker` (searchable combobox with checks).
- `Dialog` (header, scrolling body, footer with Cancel then primary; `bare` for the palette).
- `.badge`, `.card`, `.switch`, `.keycap`.

**Form controls** (`web/src/components`, styles in `styles/controls.css`):

- **`Select`** replaces `<select>`.
  - It takes `options`, or `<option>`/`<optgroup>` children, and `onChange` gets the value.
  - Keyboard as a native select: arrows, Home/End, PgUp/PgDn, Enter/Space, Esc, and type-ahead.
  - A search row appears past eight options; `required` still blocks the form.
  - Tests use `choose(locator, value)`.
- **`DateTrigger` / `DateField` / `DateRangeField`** replace the browser calendar. The panel has:
  - a typed field that understands both languages (`2026-11-30`, `11/30` or `30/11` by language, `nov 30`, `besok`, `+3d`, `next fri`, `senin depan`; `web/src/lib/dates.ts`);
  - quick picks;
  - a month grid with month and year views; the week starts Sunday in English and Monday in Indonesian;
  - a footer that says what the date means ("Due in 8 days", "Overdue by 4 days").

  The range panel adds two months, quick ranges, "like the last cycle", and a warning when a cycle would overlap another one. Tests use `pickDate` / `pickRange`.
- **`NumberField`:** − and + around a number input whose spinner is hidden.
- **`ColorButton`:** a swatch that opens a saturation and brightness square, a hue slider, hex, eyedropper and the palette.
- **`FileDrop`:** a dashed glass area to drop a file on.
- **Checkbox and radio:** the native input with our look (`appearance: none`).

React wrappers in `web/src/components/ui.tsx`.

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
