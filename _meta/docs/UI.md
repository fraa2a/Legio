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
| Container padding | compact 16, wide 24 px | `p-4`, `p-6` |
| Vertical rhythm | related 8, row 16, group 24 px | `gap-2`, `gap-4`, `gap-6` |
| Motion | fast 120, base 200, slow 320 ms, one curve | `--legio-motion-fast`, `--legio-motion`, `--legio-motion-slow`, `--legio-ease` |
| Disabled | 50% opacity | `disabled:opacity-50` |
| Focus | 2 px outline in `--legio-focus`, 2 px offset | global `:focus-visible` rule |

Corner and density options change `--radius-*` and `--spacing-control-*` in one place.

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

Transitions use `--legio-motion` and `--legio-ease` by default; zoom and layout changes
use `--legio-motion-slow`. Dialogs, toasts and menus take 200 ms. Reduced motion
removes transitions. Visually hidden inputs, such as segmented options and theme
tiles, draw the same focus outline on their visible control.
