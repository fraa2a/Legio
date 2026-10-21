# Troubleshooting

## Report a problem

1. Check [Known issues](KNOWN-ISSUES.md).
2. Enable the relevant logging in Settings, fully quit Legio (including the tray), then restart and reproduce.
3. Open a [bug report](https://github.com/fraa2a/Legio/issues/new?template=bug.yml) with:
   - Legio version, OS and package type.
   - Steps, expected result and actual result.
   - Relevant logs from the directory shown in Settings.

For graphics issues, also include GPU/driver, display scaling, Wayland/X11 and WebView2 or WebKitGTK version. See [Linux rendering diagnostics](LINUX-RENDERING.md) for backend comparisons.

**Before sharing:** remove credentials, private URLs and personal data. Report vulnerabilities privately through [SECURITY.md](../../SECURITY.md).

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
