# Legio UI

The visual scale shared by every view. Tokens live in [app.css](../../src/app.css);
primitives in `src/lib/components/ui/` apply them. `corepack pnpm test:ui-spec`
([test_ui_spec.mjs](../../scripts/test_ui_spec.mjs)) checks radii, colors and control
heights in `.svelte` files.

## Scale

| Category | Values | Tokens or classes |
| --- | --- | --- |
| Radius | control 8, card 12, panel 16 px, pill | `rounded-control`, `rounded-card`, `rounded-panel`, `rounded-full` |
| Control height | sm 32, md 40, lg 48, hero 60 px | `h-control-sm` ... `h-control-hero` (`--spacing-control-*`) |
| Icon | sm 14, md 16, lg 20, xl 24 px | `Icon size="sm"` ... `"xl"`, default `md` |
| Text | note 12, body 14, section 16, dialog 18, page 24 px | `text-xs`, `text-sm`, `text-base`, `text-lg`, `text-2xl` |
| Weight | body 400, label 500, title 600 | regular, `font-medium`, `font-semibold` |
| Text color | fg, fg-muted, fg-subtle | `text-legio-fg`, `text-legio-fg-muted`, `text-legio-fg-subtle` |
| Lines | subtle 10%, default 15%, strong 30% of the text color | `border-legio-line-subtle`, `border-legio-line`, `border-legio-line-strong` |
| Fills | fill 5%, hover 10%, active 15% of the text color | `bg-legio-fill`, `bg-legio-fill-hover`, `bg-legio-fill-active` |
| Container padding | compact 16, wide 24 px | `p-inset-compact`, `p-inset-wide` (`--spacing-inset-*`) |
| Vertical rhythm | related 8, row 16, group 24 px | `gap-2`, `gap-rhythm-row`, `gap-rhythm-group` (`--spacing-rhythm-*`) |
| Motion | fast 120, base 200, slow 320 ms, one curve | `--legio-motion-fast`, `--legio-motion`, `--legio-motion-slow`, `--legio-ease` |
| Disabled | 50% opacity | `disabled:opacity-50` |
| Focus | 2 px outline in `--legio-focus`, 2 px offset | global `:focus-visible` rule |

Corner, density and text scale options change these tokens in one place; see
[Appearance scale](#appearance-scale).

## Appearance scale

`applyAppearance` in `src/lib/services/appearance.ts` sets four unitless multipliers on
`:root` from the appearance settings, and `@theme` in `app.css` multiplies the tokens by
them. Every default is 1, so the default look is the table above.

| Option | Values | Variable | Scales |
| --- | --- | --- | --- |
| Corners | sharp 0.25, soft 1, round 1.75 | `--legio-corner-scale` | `--radius-control`, `--radius-card`, `--radius-panel` |
| Density | compact, normal, wide | `--legio-control-scale` (0.875, 1, 1.125) | `--spacing-control-*` |
| Density | compact, normal, wide | `--legio-space-scale` (0.75, 1, 1.25) | `--spacing-inset-*`, `--spacing-rhythm-*` |
| Text size | 90 to 120 percent | `--legio-text-scale` | `--text-xs` to `--text-4xl` |

- Container padding and rhythm are tokenized in the primitives of `components/ui` only; feature
  views that still use `p-4`, `p-6` or `gap-4` keep their fixed spacing.
- Text size scales font sizes only, never spacing or control heights. It is independent of the
  window zoom of `window_scale.rs`, which scales the whole page on small windows, so the two multiply.
- The three options live in the appearance settings JSON (`density`, `corners`, `textScale`), are
  validated in `appearance.rs` and travel with theme export and import. Missing or unknown values
  load as the defaults.
- A new scaled token goes into `@theme` with a `var(--legio-*-scale, 1)` factor, not into a component.

## Palettes, per-game accent and sharing

- Palettes are entries in `src/lib/services/theme-presets.json`, with an optional OKLCH `tint`
  for the neutral ramp, and a matching `Theme` variant in `database.rs`. Every palette keeps text
  and muted text at 4.5:1 or more on the background and the surface (`test:appearance`); high
  contrast targets 7:1.
- The option `gameAccent` (off by default) makes a game page take the dominant vivid color of its
  hero or custom banner. It is sampled in the frontend from the loaded image
  (`services/game-accent.ts`), because the artwork pipeline caches several derived images and a
  custom banner never passes through it. `adjustAccent` moves the lightness in OKLCH until the
  color has 4.6:1 on the background and the surface, so accent text and focus rings stay readable.
  `App.svelte` passes it to `applyAppearance`; `--legio-accent`, `--legio-accent-text` and
  `--legio-focus` are registered properties that fade over `--legio-motion-slow` only while the
  option is on, and not at all under reduced motion. Late loads of other artwork are dropped.
- A theme is shared as `LT1.` plus base64url of the minified profile without the wallpaper, up to
  4096 characters. Rust (`appearance.rs`) validates codes like imported files and rejects other
  versions. `legio://theme?code=<code>` opens `ThemeLinkDialog`, which previews the theme live and
  applies it only after confirmation, like source links.

## Radius

- Control: buttons, fields, menu items and icon buttons of every size.
- Card: game tiles, banners, toasts, inner groups and hero controls.
- Panel: `Panel`, `SettingsGroup` and dialogs.
- Pill: badges, labels on artwork and toggles.

A nested radius is the outer radius minus the padding, as in `SegmentedControl`
(card 12 with 4 px padding, items at 8).

## Controls

| Size | Height | Text | Icon | Use |
| --- | --- | --- | --- | --- |
| `sm` | 32 px | `text-xs` | 16 px | Toolbars and dense rows |
| `md` | 40 px | `text-sm` | 16 px | Default and forms |
| `lg` | 48 px | `text-base` | 20 px | Sidebar and main actions |
| `hero` | 60 px | `text-xl` | 24 px | Play and download on the game page |

- `Button` and `IconButton` share `sm` and `md`. `IconButton` also has `inline-sm` (20 px)
  and `inline-md` (28 px) for actions inside another control, such as a tab or a banner.
- Toolbars are `md` high with `p-1` and `sm` buttons.
- `Button` shapes: `rect` on panels, `pill` or `circle` over artwork. A circle is
  icon-only; `iconOnly` makes a square `rect` or `pill`.
- Text-only actions use `Button` `ghost`, or `link` inside banners and toasts.
- Use `TextField` (`type="search"` adds the icon, `variant="toolbar"` the glass surface,
  `type="textarea"` for code), `SelectField`, `SegmentedControl`, `RadioCard`, `Slider`
  and `Toggle` instead of raw inputs. Clickable tiles and the video player surface stay raw.

## Title bar

`TitleBar.svelte` is a context bar of `md` glass groups, left to right:

- **Path**: the current page, after a back crumb when there is one (`Libreria / Game`,
  `Store / Game`, `Mod / Game`, `Game / Mod`). `titleBarPath` in `title-bar-model.ts` derives it
  from the navigation stores. The back crumb and Escape run the same back navigation. Names
  truncate with a `title`. Page controls from `showTitleBarContent` follow the path (`start`).
- **Search**: in the library and store lists, the `#title-bar-search` field filters the list.
  Elsewhere a search-styled button opens the command palette; a key typed on it starts the
  palette query. Ctrl+F focuses the filter or opens the palette.
- **Status**: page `end` controls, the download status (count, progress and speed while
  downloads run; it opens Downloads), the notification bell, then the window controls.

Every non-interactive wrapper carries `data-tauri-drag-region`, since only those elements drag
the window. When space runs out the search narrows first and keeps only its icon under 12rem,
then the path truncates; the right side never shrinks. Below a 72rem bar the download status
shows only its icon and count.

The bell box turns into the tab of its panel, which grows from it as one surface like a
`SelectField` list. The panel lists the session history of `stores/notifications.ts`: every
toast plus background outcomes (download finished or failed, update available, source refresh
failure), at most 100 entries in memory. Opening it marks them read; a retry runs once and is
dropped when it no longer applies.

## Typography

| Style | Classes | Use |
| --- | --- | --- |
| Page title | `text-2xl font-semibold` | Game name, view title |
| Dialog title | `text-lg font-semibold` | `Dialog`, `EmptyState` |
| Section title | `text-base font-semibold` | `Panel` |
| Group title | `text-sm font-semibold` | `SettingsGroup`, list headings |
| Label | `text-sm font-medium` | Field labels, buttons, navigation |
| Body / note | `text-sm` / `text-xs` muted | Text and descriptions |

`semibold` is for titles, `medium` for what is clicked or labels a field, regular for
the rest. Only the hero button is `bold`.

## Color

Neutrals come from zinc, white and black, recolored by the theme through the `legio-*`
tokens. Tone colors (`legio-info`, `-success`, `-warning`, `-error`) and action colors
(`legio-play`, `-download`, `-danger`) are tokens too. White and black stay for text,
scrims and shadows on artwork and video; no other Tailwind palette is used.

## Icons

One icon per concept. A settings group without an icon of its own has no badge.

## Spacing

Compact containers (callouts, toasts, cards) use 16 px padding; wide ones (panels,
settings groups, dialogs, empty states) use 24 px. Half steps (`gap-1.5`, `gap-2.5`)
only separate an icon from its text.

## Motion and states

Pressing Play expands the cover once (`.legio-cover-launching`) while the single launch button
moves its color from `legio-play` through `legio-launching` to `legio-danger` over `--legio-motion-slow`.
Loading states use `Skeleton` blocks with the size of the real layout inside a `role="status"`
`aria-busy` block with hidden text; the pulse stops under reduced motion.

Transitions use `--legio-motion` and `--legio-ease` by default; zoom and layout changes
use `--legio-motion-slow`. Dialogs, toasts and menus take 200 ms. Reduced motion
removes transitions. Visually hidden inputs, such as segmented options and theme
tiles, draw the same focus outline on their visible control.

## Accessibility

Contrast targets, measured by `test:appearance` for every palette from the token values in
`app.css` (color-mix tokens are resolved numerically):

| Pair | Minimum |
| --- | --- |
| `fg`, `fg-muted` on background and surface | 4.5:1 (7:1 for high contrast) |
| `fg-subtle` on background and surface | 4.5:1 |
| `info`, `success`, `warning`, `error` on background, surface and their 10% tint (`Callout`, toasts) | 4.5:1 |
| `accent-text` on `accent` | 4.5:1 |
| White label on `download`, `play`, `launching`, `danger` and their hover colors | 4.5:1 |
| Focus ring (`--legio-focus`) on background and surface | 3:1 |
| `line-control` (border of `TextField` and `SelectField`) on background and surface | 3:1 |

- `fg-subtle` is muted text dimmed 10% toward the ground it sits on, so it stays readable on
  light palettes. It is real text (hints, placeholders), never decoration.
- Lines and fills are decorative and have no target. A control whose only boundary is a line uses
  `border-legio-line-control`.
- Focus: every interactive element shows the global 2 px `--legio-focus` outline on keyboard
  focus. Hide an input and draw the outline on its visible control (`peer-focus-visible`,
  `has-focus-visible`); never remove it. The outline is suppressed only after a pointer press.
- Semantics: icon-only buttons need `label`; toggles are `role="switch"`; segmented controls and
  radio cards are native radio groups; no positive `tabindex`; decorative artwork has `alt=""`;
  `<html lang>` follows the app language.
- Automatic checks: `test:a11y` mounts the primitives in JSDOM (accessible names, dialog modal,
  label, Escape and focus return, native radio and range semantics) and scans sources for positive
  `tabindex` and images without `alt`. JSDOM cannot check real focus order, the top layer or
  screen reader output; use the pass in [TESTING.md](TESTING.md).
