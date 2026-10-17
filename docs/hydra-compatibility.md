# Hydra compatibility implementation status

PR #208 implements the approved HTTP archive scope. The selection policy below replaces the earlier GoFile-first speed ranking at the user's request on 2026-10-17.

## Implemented

- Detect native Legio and Hydra source formats without falling back from malformed native documents.
- Import Hydra records as unverified, preserving publisher name, original records and candidate links.
- Match Steam titles conservatively, preserve sequel/edition markers, and cache successful matches. Remote matching has bounded concurrency and request rate; repeated catalog failures stop further lookups until refresh.
- Preserve separate source releases and one Store result per Steam App ID.
- Display publisher-name badges in version choices, import diagnostics and matching progress.
- Provide automatic or manual link selection within the selected source release.
- Preserve JSON link order within the selected release. Attempt each link once, then retry it up to three times before advancing. GoFile receives no automatic priority. An explicit manual link choice moves that link to the front of the same fallback list.
- Persist link order, active mirror and failed-attempt count in each download, including pause, resume and application restart. Exhaustion produces a failed job rather than an unlimited connectivity retry loop. An explicit Retry starts a fresh bounded cycle.
- Reset partial bytes, ETag and progress before changing mirrors. Recheck the next mirror's exact size and preserve native published-size constraints.
- Keep native HTTPS downloads from `cdn.taxphobia.top` on the existing resumable transfer path. No multipart-archive feature or replacement CDN downloader is introduced.
- Validate other links during source import/refresh with bounded concurrency, exact-size checks and a small archive-signature read. Omit unsupported or unavailable links and omit entries with no remaining links. Persist validated sizes and filtered Hydra records so omitted entries do not return from cache. Queueing uses this metadata immediately; old Hydra caches without it request a source refresh instead of running an uncancellable preparation probe.
- Resolve supported single-file HTTP downloads when the host supplies an exact size. Re-resolve host links at transfer/resume and retain source provenance.
- Validate redirect destinations, strip sensitive headers across origins, reject HTML/JSON download responses and preserve native size/hash checks.

## Provider status

Host protocols are covered by automated fixtures. Every imported non-CDN link is also validated against its actual response by the application; a recognized host name alone does not make an entry downloadable. External providers can change or reject requests, in which case that link is omitted at refresh or exhausted through the bounded transfer fallback.

| Provider | Checkpoint |
| --- | --- |
| Direct HTTP(S) | Integrated with the existing single-file queue and validated redirects. |
| PixelDrain | File URL resolution plus import-time archive validation. |
| MediaFire | Direct-link extraction plus import-time archive validation. |
| FuckingFast | Direct-link extraction plus import-time archive validation. |
| Rootz | Page-token/API resolution plus import-time archive validation. |
| GoFile | Hydra guest/session flow ported: dynamic website-token script execution, persistent guest tokens, auth refresh, retries, pagination, nested-folder lookup, URL passwords and HEAD validation. Selects the first accessible file as Hydra does, with its alternate CDN fallback and 15-second protocol cache. The resulting file must pass import-time archive validation. |
| Torrent/magnet | Temporarily excluded from the first release at the user's request on 2026-10-17. Links remain recognized as unsupported. |
| Paid providers or private Hydra unlock APIs | Excluded from this feature. |

Anonymous guest tokens used internally by a free host are distinct from user-provided paid API keys. No paid account configuration has been added.

## Approved transfer scope

- Download ZIP, 7z and RAR archives that resolve to one file with a known exact size. Reuse Legio's installation and extraction flow.
- Omit standalone installers, unknown-length responses, grouped/multipart archives and unsupported landing pages. Do not present an entry as downloadable when all its links are omitted.
- Torrent/magnet and paid/private Hydra backend integrations remain excluded. DataNodes and VikingFile landing-page resolvers are not added; FileKeeper and AkiraBox landing pages have no supported resolver. This is not a statement that those websites only offer paid downloads.
- Keep one Store game per Steam App ID and keep publisher-specific versions separate. The ordered fallback stays inside the chosen release; it does not silently switch to a different build or publisher.
- Source refresh retries link validation and can restore a previously unavailable entry. Existing native CDN transfers keep their previous behavior.
- Database schema 31 adds the persisted mirror plan without changing the application version.

## Sequential download validation, 2026-10-17

- All 158 frontend tests pass. Svelte check reports no errors or warnings; the frontend production build passes.
- Local HTTP fixtures verify archive-versus-HTML detection, JSON ordering, three retries after the initial attempt, mid-stream failure, discarding previous-mirror bytes, different fallback sizes, and final failed state after all links fail.
- Source fixtures verify unsupported-only entry removal, mixed valid/invalid links, validated-cache restoration and the native CDN bypass.
- The full local Rust run reports 561 passed, 4 ignored and the existing `steam_assets::tests::missing_header_waits_for_corrupt_metadata_recovery` timeout, previously reproduced on unchanged main. GitHub CI remains authoritative for unrestricted Linux and Windows validation.

## Validation record

- Local frontend production build, Svelte/TypeScript check, ESLint, and source/catalog/Store/version-badge tests passed at the initial checkpoint.
- Initial full Rust run: 492 passed, 32 failed, 4 ignored. All 32 reported failures hit sandbox-denied localhost sockets or a read-only path. This is not a passing suite.
- Follow-up checks passed for database migrations, native/source caches, Hydra parsing, release identity, title matching, provider parsing and ranking. Clippy passed after the checkpoint fixes.
- Earlier implementation CI at `0d713a8198e287545ff19134a6521b5ab67a77e0` passed all four jobs: Frontend, Rust, Tauri smoke (Linux), and Tauri smoke (Windows). Clippy passed with warnings denied. The full Linux Rust suite reported 533 passed, 0 failed and 4 ignored, including all seven GoFile protocol tests. Linux AppImage and Windows release builds completed. These unrestricted results supersede the initial sandbox-only failures above.
- Evidence: https://github.com/fraa2a/Legio/actions/runs/39053972352
- CI remains authoritative for the unrestricted Linux/Windows checks. The later sequential-download scope and its final CI results supersede this implementation checkpoint.

## GoFile protocol port, 2026-10-17

The port follows Hydra at commit `a22ec02201e73db8ca2ee214c2b37cd2d6a27374`, including `hosters/gofile.ts` and the official-first/HEAD-check/CDN-fallback sequence in `download/download-manager.ts`.

- GoFile website script discovery with the same two fallback script URLs.
- Execution of `generateWT` with Hydra's probe identity and a replaced SHA-256 function to capture the current secret.
- SHA-256 website tokens using the same user agent, language, account token and four-hour time slot.
- Guest-account persistence under the active application's data directory, serialized authorization and one refresh after a rejected account token.
- Three attempts for retryable HTTP/rate-limit responses, paginated and cycle-bounded folder traversal, password hashing, first accessible file selection and account cookie on file requests.
- Official file HEAD validation before trying Hydra's alternate CDN, whose probe uses a five-second deadline and a 15-second positive/negative cache.

Rust uses an embedded QuickJS runtime instead of Node's VM. Script execution has a one-second deadline, 16 MiB memory limit and no filesystem, process, network, module-loader or Tauri bindings. Folder traversal is limited to 100 content requests and overall resolution to 120 seconds. These resource limits are deliberate adaptations to the native launcher. Automatic selection allows up to 120 seconds for GoFile initialization instead of cancelling its website/session flow after the eight-second budget used for other hosts.

The local missing-feature regression was observed failing before implementation. The shared JavaScript probe fixture passes under Node, including timeout and absent host APIs. Local Rust compilation is blocked because this environment cannot download the newly added runtime crates from crates.io. The unrestricted CI completed successfully for Linux and Windows as recorded above. Live host acceptance remains a separate, unverified check.

Torrent support remains excluded. DataNodes delegates to an authenticated private Hydra backend, and VikingFile's Hydra resolver requires a subscription. Those backend implementations are not supplied by the MIT desktop client; this port does not add Hydra credentials or paid integrations.

## Integration decisions

- An isolated local clone was used because the managed worktree tool failed. The original checkout was preserved.
- The approved plan excludes paid integrations; no paid services or credential UI were introduced.
- The cache representation uses a common envelope around the existing native manifest types. Schema version 30 prevents older builds from opening the incompatible cache representation; old native cached documents remain readable by the new code.
- Main's download fixes from #207 were integrated, including its removal of the installed-game source-binding settings panel.
- Work was performed directly without subagents. The implementation has an author review, not an independent review.

## Host pricing clarification, 2026-10-17

A free website download, a free developer API, and a free Hydra resolver are separate capabilities. The current unsupported classification groups backend-dependent hosts with paid integrations; it must not be interpreted as a claim that those websites only offer paid downloads.

| Host | Official information checked |
| --- | --- |
| DataNodes | Free downloads with waits/ads exist. Its developer API requires a personal API key. The inspected Hydra client delegates resolution to Hydra's authenticated backend without an explicit subscription flag. |
| VikingFile | Regular mode is free with ads and a landing page. Premium offers direct downloads. The inspected Hydra resolver explicitly requires a Hydra subscription. |
| FileKeeper | Its free tier allows downloads without an account. Paid plans add features. |
| AkiraBox | Free visitor/registered tiers exist. The pricing page marks API uploads as Premium-only; this does not establish that downloads require payment. |

GoFile's published developer documentation says most API endpoints require Premium. Hydra's guest downloader instead follows the website's guest-account/session flow and supplies a website token. The alternate `https://gofilecdn.eu.cc` endpoint is present in Hydra's code, but its operator and official relationship with GoFile have not been verified. It must not be described as a guaranteed free official GoFile API/CDN.

References:

- https://gofile.io/api
- https://gofile.io/premium
- https://datanodes.to/premium
- https://datanodes.to/pages/api
- https://vikingfile.com/
- https://filekeeper.net/premium
- https://akirabox.com/premium
- https://akirabox.com/developers
- https://github.com/hydralauncher/hydra/tree/a22ec02201e73db8ca2ee214c2b37cd2d6a27374/src/main/services/hosters

## SteamGG title normalization, 2026-10-17

Hydra's current desktop client posts source URLs to `/download-sources` on its backend with `needsAuth: false`. Its shared `formatName` performs character folding, case/symbol/spacing cleanup and edition cleanup, but the source-to-game assignment is not implemented in that public client. The older v2.0.3 `RepacksManager` indexed formatted release titles with FlexSearch and searched that index using catalogue titles. Token search can find a release despite extra marketing words, unlike Legio's exact local title index. This does not establish that every source record has a correct Steam association.

Legio now strips recognized release-page labels such as `Free Download`, common spelling variants, `Direct Download`, `Download Free` and leading `Download`. It handles bracketed versions, builds, hotfix/build payloads, version separators, apostrophes, trademark symbols and Hydra's Latin character folding map. Explicit edition, remaster, sequel and VR identity is retained rather than merging those records into a base game. Original release titles and source trust remain unchanged in storage and the UI.

The user-provided SteamGG file contained 6,678 records. Running the production normalizer over all titles produced no empty names, no remaining numbered build markers, and no changes when normalizing the output a second time. One unusual `Free Download SIMULACRA` prefix remains deliberately unresolved by the generic marketing cleanup, which must preserve real names such as `Free Download Simulator`. Against the user's partial local cache of 559 catalogue games, unique exact title associations improved from 0 to 183 source records. This is an offline comparison against a partial cache, not a count of all games that can be resolved through Steam's remote search.

Nine standalone production matching tests and four Hydra parser tests pass, including sequel/edition ambiguity, Unicode and whitespace regressions. The importer collapses whitespace in release titles before rejecting other control characters, so valid build labels containing tabs are accepted. The source import/cache integration regression and full project checks run in CI. These changes improve title recognition; they do not add support for additional download hosts.

Code references:

- https://github.com/hydralauncher/hydra/blob/a22ec02201e73db8ca2ee214c2b37cd2d6a27374/src/main/events/download-sources/add-download-source.ts
- https://github.com/hydralauncher/hydra/blob/a22ec02201e73db8ca2ee214c2b37cd2d6a27374/src/shared/index.ts
- https://github.com/hydralauncher/hydra/blob/a22ec02201e73db8ca2ee214c2b37cd2d6a27374/src/shared/char-map.ts
- https://github.com/hydralauncher/hydra/blob/v2.0.3/src/main/services/repacks-manager.ts

## FlexSearch release index, 2026-10-17

Legio now embeds the actual FlexSearch library, rather than only implementing a title normalizer or a similar Rust search algorithm. The pinned upstream commit is `4c3966709f855c6980c21035cf230b2f9150d065`, whose package version is `0.7.43`, matching Hydra v2.0.3's dependency. The unmodified upstream bundle retains its older `v0.7.41` banner. Its Git blob SHA is `aba4476ddf956f12a469a906cac04e41da83576b`. Apache-2.0 attribution is retained and the full license is included in application bundles.

The index direction follows Hydra's old `RepacksManager`: index normalized release titles, then query using catalogue game names. This finds source titles with added release words and reordered words. Source result limits use the source's record count, avoiding the default top-result cap. No worker, browser, filesystem or network bindings are exposed to the embedded library.

Rust retains final authority over Steam App ID assignment. Exact normalized local matches take precedence. Text candidates must retain sequel/edition/VR tokens, contain the catalogue title's complete word set, and have only recognized release-page words left over. Distinct surviving App IDs are ambiguous and remain unresolved; duplicate rows for the same ID do not create false ambiguity. Generic title subsets such as `Portal` versus `Portal Stories Mel` are rejected. This deliberately adds safeguards beyond Hydra's historical repack search, which returned download options rather than importing a permanent Steam identity.

The local source import builds one release index for the available catalogue. Remote Steam lookup results also pass through FlexSearch when conservative matching cannot identify the release. Remote matching runs on a blocking worker, keeping JavaScript execution off the asynchronous runtime. Index errors are reported in source import diagnostics; exact local matches remain available if text indexing fails. Cached known matches do not require an index rebuild with an empty catalogue.

Index execution is limited to 128 MiB and ten seconds, with at most 100,000 releases and catalogue candidates per batch. This reuses the existing embedded QuickJS dependency and introduces no runtime dependency on Node.js or an external search service.

Offline check on the provided SteamGG file: the production parser accepts 6,654 records. Against the partial local catalogue of 559 games, exact normalization alone identifies 181 source records, and the FlexSearch candidates plus Rust guards identify 182, with no ambiguous result in that subset. A Node execution of the bundled index and production JavaScript adapter took about 50 ms for indexing and searching that subset; this is a local benchmark, not a QuickJS or full Steam catalogue performance guarantee. The earlier normalization-only count of 183 included raw records that the source parser rejects.

The JavaScript test exercises the actual vendored library for added release words, word ordering, missing titles, and more than 100 results. Rust regressions cover final assignment, generic-title rejection, sequel/VR separation, ambiguous IDs, exact-match precedence and cache restoration. Full Rust verification runs in CI because the local sandbox does not have the QuickJS crate cached.
