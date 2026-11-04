# Source identification cache

Adding a Hydra source makes Legio identify every download title against the game catalog, which is slow for large sources. This tool publishes the result once so other users skip those lookups. Clients only read it over HTTPS and fall back to local identification when the file is unreachable or invalid, or has no entry for their source.

The tool does not reimplement title matching. It exports what the app already computed from the Legio database, so the cache matches what the app would produce.

## Privacy

The file never contains a source link or any download title. Source keys and title keys are SHA-256 hashes.

## Schema v2

A single file, `https://legio-cache.fraa2a.dev/cache.json`, holds every published source. Each entry is keyed by the SHA-256 of the source link as Legio stores it (the validated URL, for example `https://example.com/games.json`).

```json
{
  "version": 2,
  "generatedAt": "2026-11-04T12:00:00Z",
  "sources": {
    "<sha256 hex of the source link>": {
      "contentSha256": "<hex>",
      "generatedAt": "2026-11-04T12:00:00Z",
      "matches": { "<sha256 hex of the raw download title, UTF-8>": 400 },
      "unmatched": ["<sha256 hex of the raw download title, UTF-8>"]
    }
  }
}
```

- `generatedAt`: UTC RFC 3339 time of export, for the file and for each entry.
- `contentSha256`: SHA-256 of the raw source bytes the app fetched.
- `matches`: title hash to Steam app id (positive integer up to 4294967295).
- `unmatched`: titles looked up successfully with no confident match. Titles whose lookup failed or was stopped are never listed.
- Hashes are 64 lowercase hex characters. The file is limited to 32 MiB.
- A source with no entry is simply not cached: the client identifies it locally.

When `contentSha256` differs from the client's copy, the client still uses the entries for titles present in its source and looks up only titles listed in neither map. The client downloads the file once per add or refresh operation.

## Update the cache

1. In Legio, add the Hydra source (Settings > Sources) and let identification finish. If you press Stop, refresh the source until every title is identified.
2. Run the tool (Python 3.10 or newer, standard library only):

```
python tools/source-cache/source_cache.py https://example.com/games.json
python tools/source-cache/source_cache.py --from-file links.txt
python tools/source-cache/source_cache.py --remove https://example.com/games.json
```

The tool loads the currently published `cache.json` (an empty cache if it does not exist yet, or the local copy given with `--base <file>`), updates the entries for the given links, drops the `--remove` ones, and writes `cache.json` (change it with `--out <file>`). Entries for other sources are kept as they are.

`links.txt` has one HTTPS link per line; blank lines and `#` comments are ignored. Links must be installed in Legio. The database is found in the app data folder (`dev.fraa2a.legio/legio.sqlite3`); use `--db <file>` to choose another. Sources that are incomplete or not Hydra are skipped with a message and a non-zero exit code; the other entries are still written.

Re-add or refresh a source in Legio before exporting again after the source changes.

## Hosting on Cloudflare R2

1. Create an R2 bucket.
2. Attach the custom domain `legio-cache.fraa2a.dev` to it so `https://legio-cache.fraa2a.dev/cache.json` is public. If the address changes, update `URL` in `src-tauri/src/identification_cache.rs` and `PUBLIC_URL` in the tool.
3. Create an R2 API token scoped to that bucket with Object Read and Write permissions.

## Credentials

Copy `r2.example.json` to `r2.local.json` in this folder and fill it in. The local file is ignored by git.

```json
{ "accountId": "", "accessKeyId": "", "secretAccessKey": "", "bucket": "" }
```

Environment variables override the file values:

| Field | Variable | Value |
| --- | --- | --- |
| `accountId` | `R2_ACCOUNT_ID` | Cloudflare account id |
| `accessKeyId` | `R2_ACCESS_KEY_ID` | R2 API token access key |
| `secretAccessKey` | `R2_SECRET_ACCESS_KEY` | R2 API token secret |
| `bucket` | `R2_BUCKET` | Bucket name |

Missing fields are reported by name. Keep the token private and never commit it.

## Upload

`--upload` puts the written file to the bucket as `cache.json` with `Content-Type: application/json` and `Cache-Control: public, max-age=3600`. It needs `boto3` (`pip install boto3`), which is not used otherwise.

```
python tools/source-cache/source_cache.py --upload --from-file links.txt
```
