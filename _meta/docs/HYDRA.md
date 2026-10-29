# Hydra source compatibility

Legio accepts native Legio catalogs and Hydra download sources. See [Source schemas](SCHEMA.md) for native fields and validation. Malformed native documents are rejected rather than interpreted as another format.

## Import and game identity

Hydra records are imported as unverified. Legio preserves publisher names, release titles, original records and candidate links. Sources remain independent; the Store groups games by Steam App ID while keeping publisher-specific releases selectable.

Title matching is conservative. Normalization handles release-page labels such as `Free Download`, build/version suffixes, whitespace, punctuation and Latin character folding. Sequel, edition, remaster and VR identity must remain distinct. Exact normalized local matches take precedence; ambiguous Steam identities remain unresolved. Successful matches are cached. Remote matching has bounded concurrency and rate limits, and repeated catalog failures suspend lookups until refresh.

The embedded FlexSearch index searches normalized release titles using catalog game names. Rust makes the final Steam identity decision: candidates must contain the complete catalog title and preserve identity tokens; remaining words must be recognized release-page labels. A broad title subset is not sufficient to assign a game.

FlexSearch is pinned to upstream commit `4c3966709f855c6980c21035cf230b2f9150d065`, package version `0.7.43`. The unmodified bundle retains its upstream `v0.7.41` banner. Its license is in `src-tauri/vendor/flexsearch.LICENSE` and [third-party notices](../../THIRD_PARTY_NOTICES.md). Index execution is limited to 128 MiB and ten seconds, with at most 100,000 releases and catalog candidates per batch. It runs on blocking workers without browser, filesystem or network bindings; no Node.js runtime is required by the application. Index failures appear in import diagnostics, while exact local matches remain available.

## Supported downloads

Supported downloads resolve to a single ZIP, 7z or RAR archive with a known exact host size. Standalone installers, unknown-length responses, grouped/multipart archives, torrent/magnet links and paid/private Hydra backend integrations are not supported.

| Provider | Resolution |
| --- | --- |
| Direct HTTP(S) | Single-file transfers with validated redirects |
| PixelDrain | File URL resolution and archive validation |
| MediaFire | Direct-link extraction and archive validation |
| FuckingFast | Direct-link extraction and archive validation |
| Rootz | Page-token/API resolution and archive validation |
| GoFile | Guest website/session flow, folder lookup and archive validation |
| 1fichier | Public link checker for size and name, guest download form for the one-time link |
| Buzzheavier | Page token and `HX-Redirect` download request, archive validation |

DataNodes, VikingFile, FileKeeper and AkiraBox landing pages do not have supported resolvers. This is not a claim that those websites require payment for all downloads. Free website downloads, public developer APIs and Hydra backend access are separate capabilities.

Every imported non-CDN link is checked against its response with bounded concurrency, size checks and a small archive-signature read. A recognized host name alone does not establish download availability. Unsupported or unavailable links are omitted; entries with no remaining links are omitted too. Validated sizes and filtered records are cached. Refresh retries validation and can restore entries. Old Hydra caches lacking validation metadata require refresh before queueing.

Native HTTPS transfers from `cdn.taxphobia.top` retain their existing resumable path. Source provenance is preserved, host URLs are resolved again when transferring or resuming, redirects are validated, sensitive headers are stripped across origins, and HTML/JSON download responses are rejected.

## Link selection and retries

Links retain their JSON order. Each link gets an initial attempt and up to three retries before advancing. GoFile has no automatic priority. A manual choice moves that link to the front of the same release's fallback list; fallback never silently switches build or publisher.

The queue persists link order, active mirror and failed-attempt count through pause, resume and restart. Exhausting the list produces a failed job; explicit Retry starts a fresh bounded cycle. Changing mirrors clears partial bytes, ETag and progress before checking the next exact size.

## 1fichier

Hydra only lists 1fichier as a host that needs a Real-Debrid account, so this resolver is Legio's own and follows the website's behavior for guests. Only `1fichier.com/?id` file pages are recognized. Its alias domains return 404 for known files and are not supported.

Reachability checks use the public link checker (`check_links.pl`), which reports the exact size and file name without using the single free download that guests get per hour. The archive type is therefore taken from the file name instead of the archive signature. Starting a transfer submits the guest download form and follows the one-time direct link, which is requested again whenever the transfer starts or resumes.

When the website asks the guest to wait between downloads, the error states the wait. Guests cannot resume transfers, so pausing and resuming within that hour fails with the same error.

## Buzzheavier

Hydra's main branch has no Buzzheavier resolver. The flow follows the bzzhr downloader proposed in Hydra pull request 2699, which was not merged, and was checked against the live website. The file page carries a tokenized download request; sending it with the `HX-Request` header answers with the direct link in an `HX-Redirect` header. Direct links on download servers such as `ts.buzzheavier.com` keep the plain direct path.

`buzzheavier.com`, `bzzhr.co` and `bzzhr.to` serve the same file ids. Every request goes to `buzzheavier.com`, because the other two domains were challenged by Cloudflare in every test. Cloudflare also challenges a share of requests from clients that cannot run its script, so each request is retried up to three times before the error reports a browser check. The page request needs a referrer from the website itself; without one the website redirects elsewhere.

## GoFile protocol

The guest flow follows the website/session behavior in [Hydra's hosters](https://github.com/hydralauncher/hydra/tree/a22ec02201e73db8ca2ee214c2b37cd2d6a27374/src/main/services/hosters), including website-token discovery, guest account persistence, authorization refresh, pagination, nested folders, URL passwords and first-accessible-file selection.

Website token scripts run in embedded QuickJS with a ten-second deadline and 16 MiB memory limit, without filesystem, process, network, module-loader or Tauri bindings. The deadline only stops runaway scripts: the obfuscated generator takes about 0.1 seconds in an optimized build and about 0.7 seconds in an unoptimized one. The extracted secret is reused for at most four hours, because the website rotates it and rejects retired ones. Folder traversal is limited to 100 content requests and overall resolution to 120 seconds. Automatic selection allows this initialization budget rather than the shorter budget used for other hosts.

Access gates and unavailable files produce specific errors: missing content, a required or incorrect password, private or expired content, files in cold storage and overloaded servers.

Official file HEAD validation precedes Hydra's alternate CDN fallback. The alternate `https://gofilecdn.eu.cc` endpoint comes from Hydra's code; its operator and official relationship with GoFile are not established. It must not be described as a guaranteed official GoFile service. External hosts can change their protocols or reject requests; validation and bounded fallback remain necessary. No user-provided paid API key configuration is included.

## Size and integrity policy

Native `sizeBytes` describes compressed archive bytes, not extracted contents. Import, queueing and mirror changes accept an inclusive range of 97% to 103% of that published value. The immutable published value is stored separately from exact host length, preventing retries from compounding the allowance. Completion, range totals and truncated responses are checked against exact host length; SHA-256 verification remains independent.

Hydra's human-readable `fileSize` is not an exact byte count. Without native `sizeBytes`, a validated exact host size is required. Extraction has separate path, entry-count and expansion-ratio protections; it does not impose a fixed 64 GiB installed-size ceiling.

## Local catalogs and mirrors

Settings accepts local JSON catalogs through a dedicated file picker. The selected absolute path is stored as a file URL and read again on refresh. Files must be regular, readable and at most 32 MiB. Remote source links remain HTTPS-only. An unreadable or removed file preserves its independent cache and displays a refresh warning.

Unverified native entries may omit hash and size; verified entries require their hash and positive byte count. Optional `download.mirrors` supports up to 99 alternatives after the primary URL and requires a stable entry ID. All candidates pass through supported-provider filtering and archive validation. Older Legio versions may not accept mirror fields or unknown native sizes; consult [Source schemas](SCHEMA.md) when publishing catalogs.
