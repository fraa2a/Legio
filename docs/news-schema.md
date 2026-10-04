# Launcher news and first-run setup

Legio reads `https://source.taxphobia.top/news.json`. Tax Cloud publishes this document from its protected News workspace. Network reads are bounded to 512 KiB, use the existing restricted HTTP client, and reject unsupported schemas or malformed articles. All content is plain text, never rendered as HTML.

```json
{
  "schemaVersion": 1,
  "generatedAt": "2026-10-04T12:00:00Z",
  "items": [{
    "id": "welcome",
    "publishedAt": "2026-10-04T12:00:00Z",
    "title": { "it": "Benvenuto", "en": "Welcome" },
    "summary": { "it": "Novità di Legio", "en": "Legio news" },
    "body": { "it": "Testo completo", "en": "Full article" }
  }]
}
```

Version 1 allows at most 100 unique IDs containing lowercase ASCII letters, digits and hyphens (1-64 bytes). Both translations are mandatory. Titles allow 160 characters, summaries 600 and bodies 12000. Control characters are forbidden except line breaks and tabs in bodies. Dates must be RFC3339 UTC ending in Z. Unknown fields are rejected. Future articles remain hidden until their publication date. The Home shows the newest 20 articles and opens their full text in a dialog.

The validated feed persists in SQLite across restarts. Home renders this cached copy immediately and refreshes it in the background when older than 15 minutes; errors preserve the last usable copy. Dates follow the system locale, text follows the launcher language.

Fresh installations open setup before Home. Linux adds runner discovery and prefix configuration; Windows omits these options. Settings and Linux compatibility defaults remain drafts until completion. Skipping retains current settings. The completion flag persists on the device, and General settings can reopen setup. Existing installations with stored preferences or games are treated as already configured; legacy settings JSON defaults to completed. A first-run window remains visible even if startup minimization was requested.
