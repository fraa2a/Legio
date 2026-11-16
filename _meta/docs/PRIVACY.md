# Privacy and network

This page lists every network destination Legio contacts, what triggers it and what is sent. It is based on the source code. The same list appears in **Settings, Privacy and network**.

## Summary

- Legio has no telemetry, analytics or crash reporting. No code or dependency sends usage statistics anywhere.
- Legio has no account and sends no account data of its own. Steam access is anonymous and never uses your Steam login.
- Two optional features have a switch in **Settings, Privacy and network**: the news feed and the automatic update check. Discord presence has its own switch as well. All three are on by default.
- Every request exposes your IP address to the host and carries a `User-Agent`. Legio uses `Legio/<version>` for its own requests and `Mozilla/5.0 Legio/<version>` for download link checks. GoFile requests use a browser-style `User-Agent`.
- Application and network logs are local files, off by default. See [Troubleshooting](TROUBLESHOOTING.md).

## Destinations

"Essential" means the feature that needs it has no switch. "On demand" means Legio only contacts the host while you use the feature. "Optional" means a switch exists.

| Destination | Host | Purpose | Trigger | Data sent | Kind |
| --- | --- | --- | --- | --- | --- |
| Steam store | `store.steampowered.com` | Game details and a connectivity check | Connectivity check at startup and on retry; details when a screen shows a game | Steam app ID and language code | Essential |
| Steam artwork | `shared.fastly.steamstatic.com`, other `steamstatic.com`, `steamusercontent.com` and `steampowered.com` hosts | Covers, logos, icons and trailers | When a screen shows a game | The image or video request | Essential |
| Steam PICS | Steam servers chosen by the `steam-client-rs` library | Artwork file names for a game | When artwork for a game is not cached (cache lasts 72 hours) | Steam app ID, over an anonymous login | Essential |
| Hydra catalog API | `hydra-api-us-east-1.losbroxas.org` | Store search and matching source titles to Steam games | Typing a search in the Store; linking or refreshing a source | The search text or game title | Essential |
| ProtonDB | `www.protondb.com` | Linux compatibility rating on the game page (unofficial endpoint) | Linux only, when a game page opens; the answer is cached for 7 days | Steam app ID | On demand |
| News | `source.taxphobia.top` | News articles on the home screen | Startup, then every 15 minutes while the home screen is visible, or the refresh button | The HTTP request only | Optional (`Novità di Legio`) |
| App updates | `github.com` (`/fraa2a/Legio/releases/latest/download/latest.json`; the download follows GitHub redirects) | Look for a new Legio version | Startup of a release build, or **Check for updates**. The download starts only when you choose to install | The HTTP request only. The endpoint URL has no version or platform placeholders | Optional (`Aggiornamenti dell'app`) |
| Discord | None. Local IPC (a Unix socket or a named pipe) to Discord desktop | Rich presence | While a game runs | Game name, session time and image URLs go to the Discord app on your computer. Discord then loads the images from the Steam CDN or `raw.githubusercontent.com` (Legio icon). Legio itself makes no request to those image URLs | Optional (`Discord Rich Presence`) |
| Download sources | URLs you add | Source manifests and game update data | Startup refresh, manual refresh | The HTTP request only. Legio ships with no source | On demand |
| Download hosts | `gofile.io`, `api.gofile.io`, `buzzheavier.com`, `1fichier.com`, `mediafire.com`, `pixeldrain.com`, `fuckingfast.co`, `rootz.so`, `cdn.taxphobia.top`, and any public host named in your sources | Link checks, link resolution and game downloads | Opening a game that has download links; starting a download | The link being checked. GoFile also creates an anonymous guest account. 1fichier receives the file ID in its link checker | On demand |
| Thunderstore | `thunderstore.io` and its subdomains | Mod catalog, package files, icons, profile codes | Using the mod features. Creating a profile code uploads that profile archive | Package and game identifiers you request; the profile archive when you create a code | On demand |
| Mod exclusion list | `raw.githubusercontent.com/ebkr/r2modmanPlus/develop/modExclusions.md` | Hide non-mod packages | Using the mod features | The HTTP request only | On demand |

## Not network traffic

- **Discord presence** talks to Discord desktop through local IPC. Whether Discord sends the data on is Discord's behavior.
- **Steam library sync** (`syncSteamGames` and the poll interval) reads Steam files on your disk. It makes no request.
- **Public address checks** (`public_address.rs`) are a guard, not a destination. They refuse download links that resolve to private, loopback or reserved addresses.
- **Deep links** (the `legio` scheme and the optional Thunderstore link handler) are registered with the operating system and are not network traffic.

## Window content policy

The Content Security Policy in `src-tauri/tauri.conf.json` limits what the interface itself can request:

- `connect-src` allows only the app IPC and the Steam hosts above. The frontend does not call `fetch`; network access goes through the Rust backend.
- `media-src` allows Steam hosts, used for trailers.
- `img-src` allows any `https:` address, because covers from sources and mod icons are shown in the interface. Image requests expose your IP address to the image host.

## Switches

| Setting | Default | Effect when off |
| --- | --- | --- |
| News (`newsEnabled`) | On | The backend makes no request to the news server and returns only already cached articles. The home panel shows a "news turned off" state. |
| Automatic update check (`automaticUpdateCheck`) | On | No update request is made at startup. **Check for updates** in General settings still works on demand. |
| Discord presence | On | No presence data is sent to Discord desktop. |

Other destinations have no switch: Steam, the Hydra catalog and Steam artwork are needed for the Store and covers, and the remaining entries are only contacted when you add a source, download a game or use the mod features. Settings are stored as one JSON document, so the two new switches need no database migration and older installs read them as on.
