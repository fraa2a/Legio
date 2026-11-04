# Source identification cache

Adding a Hydra source makes Legio identify every download title against the game catalog, which is slow for large sources. This tool publishes the result once so other users skip those lookups. Clients only read it over HTTPS and fall back to local identification when a file is missing or invalid.

The tool does not reimplement title matching. It exports what the app already computed from the Legio database, so cache files match what the app would produce.

## Privacy

Files never contain the source link or any download title. Names and keys are SHA-256 hashes.

## Schema v1

One file per source link, named `<sha256 hex of the source link>.json`. The hash is of the link as Legio stores it (the validated URL, for example `https://example.com/games.json`).

```json
{
  "version": 1,
  "contentSha256": "<hex>",
  "generatedAt": "2026-11-04T12:00:00Z",
  "matches": { "<sha256 hex of the raw download title, UTF-8>": 400 },
  "unmatched": ["<sha256 hex of the raw download title, UTF-8>"]
}
```

- `contentSha256`: SHA-256 of the raw source bytes the app fetched.
- `generatedAt`: UTC RFC 3339 time of export.
- `matches`: title hash to Steam app id (positive integer up to 4294967295).
- `unmatched`: titles looked up successfully with no confident match. Titles whose lookup failed or was stopped are never listed.
- Hashes are 64 lowercase hex characters. Files are limited to 8 MiB.

When `contentSha256` differs from the client's copy, the client still uses the entries for titles present in its source and looks up only titles listed in neither map.

## Export

1. In Legio, add the Hydra source (Settings > Sources) and let identification finish. If you press Stop, refresh the source until every title is identified.
2. Run the tool (Python 3.10 or newer, standard library only):

```
python tools/source-cache/source_cache.py --out cache/v1 https://example.com/games.json
python tools/source-cache/source_cache.py --out cache/v1 --from-file links.txt
```

`links.txt` has one HTTPS link per line; blank lines and `#` comments are ignored. Links must be installed in Legio. The database is found in the app data folder (`dev.fraa2a.legio/legio.sqlite3`); use `--db <file>` to choose another. Sources that are incomplete or not Hydra are skipped with a message and a non-zero exit code.

Re-add or refresh a source in Legio before exporting again after the source changes.

## Hosting on Cloudflare R2

1. Create an R2 bucket.
2. Enable public access: attach a custom domain, or enable the `r2.dev` URL.
3. Create an R2 API token scoped to that bucket with Object Read and Write permissions.
4. Set `BASE_URL` in `src-tauri/src/identification_cache.rs` to the public address of the `v1/` folder, for example `https://source-cache.example.com/v1/`. It must end with a slash.

## Upload

`--upload` pushes the written files to `v1/` in the bucket with `Content-Type: application/json` and `Cache-Control: public, max-age=3600`. It needs `boto3` (`pip install boto3`), which is not used otherwise. Credentials come only from environment variables:

| Variable | Value |
| --- | --- |
| `R2_ACCOUNT_ID` | Cloudflare account id |
| `R2_ACCESS_KEY_ID` | R2 API token access key |
| `R2_SECRET_ACCESS_KEY` | R2 API token secret |
| `R2_BUCKET` | Bucket name |

```
python tools/source-cache/source_cache.py --out cache/v1 --upload --from-file links.txt
```

Keep the token private and never commit it.
