# Hydra compatibility implementation status

This feature is under development in PR #208. This document records the current implementation checkpoint, not a claim of complete Hydra parity.

## Implemented

- Detect native Legio and Hydra source formats without falling back from malformed native documents.
- Import Hydra records as unverified, preserving publisher name, original records and candidate links.
- Match Steam titles conservatively, preserve sequel/edition markers, and cache successful matches. Remote matching has bounded concurrency and request rate; repeated catalog failures stop further lookups until refresh.
- Preserve separate source releases and one Store result per Steam App ID.
- Display publisher-name badges in version choices, import diagnostics and matching progress.
- Provide automatic or manual link selection within the selected source release.
- Prefer usable GoFile candidates, then rank other candidates by a bounded throughput sample. Preparation probes have a deadline; this is a speed estimate, not guaranteed final throughput.
- Resolve supported single-file HTTP downloads when the host supplies an exact size. Re-resolve host links at transfer/resume and retain source provenance.
- Validate redirect destinations, strip sensitive headers across origins, reject HTML/JSON download responses and preserve native size/hash checks.

## Provider status

No new host adapter has been verified against a live download in this environment. Recognition/extraction/ranking tests do not establish that a host's current API accepts the requests.

| Provider | Checkpoint |
| --- | --- |
| Direct HTTP(S) | Integrated with the existing single-file queue and validated redirects. |
| PixelDrain | File URL resolution implemented; live verification pending. |
| MediaFire | Supported direct-link extraction implemented; live verification pending. |
| FuckingFast | Supported direct-link extraction implemented; live verification pending. |
| Rootz | Page-token and API flow implemented; live verification pending. |
| GoFile | Partial guest API flow and Hydra's alternate CDN fallback. Current website-token protocol, token/session reuse, pagination and nested-folder lookup remain incomplete. |
| Torrent/magnet | Recognized, but the transfer engine is not integrated. |
| Paid providers or private Hydra unlock APIs | Excluded from this feature. |

Anonymous guest tokens used internally by a free host are distinct from user-provided paid API keys. No paid account configuration has been added.

## Remaining before this can leave draft

- Integrate and validate the torrent engine on Linux and Windows. Proposed dependency `librqbit 9.0.1` was absent from the local cache, and dependency registry access was unavailable.
- Complete GoFile's current website-token flow and lifecycle; verify small controlled live files.
- Support unknown-length streams, grouped files/multipart extraction, torrent payload directories and explicit installer-file handling. The current implementation requires one resolved file with a known exact size and does not automatically execute installers.
- Complete preparation cancellation, availability/throughput caching, bandwidth policy during probes, informative per-candidate outcomes, and safe transfer-start fallback.
- Add cross-source automatic selection only where equivalent release content and consent can be established. Current automatic selection uses the chosen source release's candidate links; it does not silently choose another build or publisher.
- Complete HTTP fixture and live-provider verification, Windows checks, running UI review and screenshots.

## Validation record

- Local frontend production build, Svelte/TypeScript check, ESLint, and source/catalog/Store/version-badge tests passed at the initial checkpoint.
- Initial full Rust run: 492 passed, 32 failed, 4 ignored. All 32 reported failures hit sandbox-denied localhost sockets or a read-only path. This is not a passing suite.
- Follow-up checks passed for database migrations, native/source caches, Hydra parsing, release identity, title matching, provider parsing and ranking. Clippy passed after the checkpoint fixes.
- CI remains authoritative for the unrestricted Linux/Windows checks. Keep this PR draft even if CI passes until the missing functionality above is implemented and verified.

## Integration decisions

- An isolated local clone was used because the managed worktree tool failed. The original checkout was preserved.
- The approved plan excludes paid integrations; no paid services or credential UI were introduced.
- The cache representation uses a common envelope around the existing native manifest types. Schema version 30 prevents older builds from opening the incompatible cache representation; old native cached documents remain readable by the new code.
- Main's download fixes from #207 were integrated, including its removal of the installed-game source-binding settings panel.
- Work was performed directly without subagents. The implementation has an author review, not an independent review.
