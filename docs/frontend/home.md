# Home frontend

The Home page follows the supplied visual reference with a last-played banner,
recent library games, download status, and a monthly activity calendar.
Achievements and progress comparisons are outside this implementation.

Preview screenshots use fixture data:
[dark theme](screenshots/home-dark.png), [light theme](screenshots/home-light.png).

## Behavior

- The banner selects the most recently played installation from real session timestamps and shows its date, name, artwork, short description, and recorded playtime.
- Continue playing reuses the existing launch lifecycle, including cancellation, stop, and Steam account-switch confirmation. Info opens game details; Settings opens the game settings dialog.
- The calendar starts on Monday, leaves slots outside the selected month blank, and uses five or six weeks as needed. Previous months are navigable; future months are disabled.
- Color intensity increases continuously from gray at zero hours to the full accent color at eight hours, with no hatching or fixed ranges. Dates, tooltips, a gradient legend, the monthly total, and active-day count accompany the cells.
- Local-day boundaries include daylight saving changes. Rust splits stored sessions into daily totals; active sessions count through request time. Concurrent games contribute separately, matching lifetime totals.
- Loading, empty, cached/offline, and retryable error states use existing frontend resources. Playtime and activity refresh every 30 seconds while Home is mounted.
- Keyboard Tab navigation is enabled; existing focus indicators and reduced-motion rules apply.

## Verification

Run `pnpm check`, `pnpm lint`, `pnpm build`, and `pnpm test:home`.
Rust activity tests cover midnight splits, active sessions, future days, and input validation.
Browser smoke checks cover launch/stop, settings, game details, Steam account confirmation,
month navigation, empty/error/retry states, keyboard navigation, themes, and responsive layout
using fixture data. Actual Steam and manual-game launches still need platform verification.
