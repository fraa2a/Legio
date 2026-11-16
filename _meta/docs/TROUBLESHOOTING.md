# Troubleshooting

## Report a problem

1. Check [Known issues](KNOWN-ISSUES.md).
2. Enable the relevant logging in Settings, fully quit Legio (including the tray), then restart and reproduce.
3. Open a [bug report](https://github.com/fraa2a/Legio/issues/new?template=bug.yml) with:
   - Legio version, OS and package type.
   - Steps, expected result and actual result.
   - The support bundle (see below), or relevant logs from the directory shown in Settings.

For graphics issues, also include GPU/driver, display scaling, Wayland/X11 and WebView2 or WebKitGTK version. See [Linux rendering diagnostics](LINUX-RENDERING.md) for backend comparisons.

**Before sharing:** remove credentials, private URLs and personal data. Report vulnerabilities privately through [SECURITY.md](../../SECURITY.md).

## Support bundle

In Settings, Advanced, choose **Crea pacchetto di supporto** (Create support bundle) and pick where to save the zip. It works with logging disabled, but then it contains no logs: enable them first and reproduce the problem to get useful ones.

The zip holds the app version and build, OS, architecture and desktop session, a fixed list of non-sensitive settings, the database schema version with row counts per table, and the newest 256 KiB of each application and network log with the home folder and user name replaced. It never includes game names, library content, download sources, account names, Steam ids, file paths or database rows. `README.txt` inside lists exactly what is in it. Nothing is uploaded: open the zip, check it, then attach it to the issue.

## Logs

Application and network logging are separate and off by default. Application logs use `app.txt` and `app.previous.txt`, up to 256 KiB each. Turning logging off preserves existing files.

On Windows, installed-build logs are in `%LOCALAPPDATA%\dev.fraa2a.legio\logs`. Development builds use a [separate profile](DEVELOPMENT.md).

A crash may leave no final log entry. Review logs before sharing, even with redaction enabled.

<details>
<summary>Windows crashes: collect events or a dump</summary>

In Event Viewer, find Application Error **1000** or Windows Error Reporting **1001** near the crash. Include the faulting module and exception code. Keep `Report.wer` and any referenced dump; RestartManager events alone do not identify the cause.

If no dump exists, download [ProcDump](https://learn.microsoft.com/en-us/sysinternals/downloads/procdump), close Legio and run from its folder in PowerShell:

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\Desktop\LegioDumps"
.\procdump64.exe -ma -e -w legio.exe "$env:USERPROFILE\Desktop\LegioDumps"
```

Start Legio and reproduce. **Share dumps privately:** they contain process memory. Debug symbols must match the exact executable build.

</details>

## Database does not open

If the local database cannot be opened or updated, Legio shows a recovery screen instead of the library. It shows the error (select it or use **Copy** to include it in a report) and offers:

- **Open data folder** to inspect `legio.sqlite3` and the `database-backups` folder.
- **Restore backup** to replace the database with one of the copies Legio makes before each schema update.
- **Use a new database** to start with an empty library.

Restoring and resetting never delete the current database: it is renamed to `legio-broken-<timestamp>.sqlite3` in the data folder, together with its `-wal` and `-shm` files. Legio then restarts.

## Moving data to another PC

**Settings > Advanced > Data** exports the library, playtime, settings, custom game icons and banners and the theme background to one `.zip` file, also useful as a manual backup. Caches, downloads, logs, mod files and installed games are not included.

**Import** checks the file (format, size, entry names and database integrity) and refuses exports from a newer Legio. After confirmation Legio restarts and applies it before opening the database. The replaced database is kept as `legio-replaced-<timestamp>.sqlite3` and the replaced images in `legio-replaced-files-<timestamp>` in the data folder. Install folders recorded on another PC may not exist here; those games need to be reinstalled or linked again. If the import cannot be applied, the current data stays in place and a warning is shown at startup.
