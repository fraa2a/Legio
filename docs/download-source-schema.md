# Download source JSON schema

## Status

This document defines the external, versioned JSON contract Legio will consume for download source metadata. It is independent from Hydra catalog data and Steam Store metadata.

The parser and installer integration are Phase 04 and Phase 05 work. This document defines the provider contract only. A source entry never makes a game catalog entry, download, or installation appear verified by itself.

## Transport and cache

- The manifest is retrieved only from the project-controlled Legio source URL.
- Retrieval requires HTTPS.
- Legio retains the last successfully validated manifest atomically.
- If refresh fails, Legio may use the last valid cache and must show that it is stale.
- A malformed, incomplete, or conflicting manifest must not replace the last valid cache.

## Top-level document

```json
{
  "schemaVersion": 1,
  "generatedAt": "2026-09-22T00:00:00Z",
  "verified": [],
  "unverified": []
}
```

| Field | Type | Requirement |
| --- | --- | --- |
| `schemaVersion` | integer | Required. Exact supported version. Version `1` is the first contract. |
| `generatedAt` | RFC 3339 UTC string | Required. Timestamp of the manifest publication. |
| `verified` | array of source entries | Required. Entries reviewed by Legio staff. |
| `unverified` | array of source entries | Required. Entries that users may choose to install despite a warning. |

`verified` and `unverified` are distinct lists. A Steam App ID may appear in at most one list. `verified` means `Verified by Legio Staff`; it is not a safety guarantee.

## Source entry

```json
{
  "steamAppId": 400,
  "name": "Portal",
  "release": { "version": "1.0.0", "publishedAt": "2026-09-22T00:00:00Z" },
  "download": {
    "url": "https://downloads.example.invalid/portal-1.0.0.zip",
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "sizeBytes": 123456789
  }
}
```

| Field | Type | Requirement |
| --- | --- | --- |
| `steamAppId` | positive integer | Required. Steam identity used to merge the source with catalog metadata. |
| `name` | nonempty string | Required. Release display name. |
| `release.version` | nonempty string | Required. Source release version. Ordering rules are a later update-policy decision. |
| `release.publishedAt` | RFC 3339 UTC string | Required. Publication timestamp. |
| `download.url` | URL string | Required. Direct HTTP or HTTPS archive URL. |
| `download.sha256` | 64 lowercase hexadecimal characters | Required. Archive integrity hash checked before extraction. |
| `download.sizeBytes` | positive integer | Required. Expected archive size for progress and disk-space checks. |

No arbitrary script, executable command, torrent, encrypted archive password, multipart archive description, or installation instruction is permitted in this contract.

## Validation rules

Legio rejects the whole refresh when any rule fails:

1. Unknown `schemaVersion`.
2. Duplicate `steamAppId` within either list or across both lists.
3. Missing required field, incorrect type, empty required string, invalid timestamp, or invalid SHA-256 encoding.
4. Non-positive App ID or size.
5. Download URL outside HTTP or HTTPS.
6. Conflicting entries for the same App ID.

Legio must preserve the previous valid cache when a refresh is rejected and surface a useful diagnostics message without disclosing sensitive local data.

## Consumer behavior

- Hydra provides catalog search and listing only.
- Steam Store provides individual-game details and Steam-hosted artwork only.
- This manifest provides Legio download source metadata only.
- The UI presents verified and unverified source status separately.
- An unverified entry presents an actionable warning but may still be installed after user choice.
- A catalog game without a valid matching entry displays `Download unavailable`.
