# Home frontend

Development scope for the Home page, based on the Home requirements in
[`PLAN.md`](../../PLAN.md) and the existing frontend
[`TODO.md`](TODO.md).

## Implementation checklist

- [ ] Reuse the existing shell, UI primitives, Tailwind tokens, and light/dark themes.
- [ ] Show Continue Playing and recently played library games using real last-played timestamps.
- [ ] Show most played games and total recorded playtime from `get_playtime_summaries`.
- [ ] Show active game state from the existing launch store and open game details through shared navigation.
- [ ] Show download queue status and provide navigation to Downloads.
- [ ] Handle loading, empty, error, and cached/offline states using the existing stores and retry controls.
- [ ] Verify responsive layout, keyboard navigation, accessible labels, and reduced-motion behavior.
- [ ] Run `pnpm check`, `pnpm lint`, and `pnpm build`; verify the page in the Tauri app.

## Data constraints

Keep Tauri calls in the existing service modules and business logic in Rust.
The current playtime summary exposes totals, active-session counts, and
last-played timestamps. Recent-period totals and the monthly activity heatmap
require session history or per-day aggregates from the backend before they can
be implemented. Do not generate placeholder activity or infer daily playtime
from lifetime totals.

Confirm the Home visual reference before final visual acceptance. The existing
Home currently displays library, download, network, and source summaries.
