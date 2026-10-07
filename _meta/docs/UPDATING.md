# Releases and updates

Increasing the version in `src-tauri/Cargo.toml` on `main` triggers a release. The pipeline builds and signs Ubuntu (`.deb`), Fedora (`.rpm`), Linux (`.AppImage`), and Windows (`.exe`) packages, then publishes their `.sig` files, `SHA256SUMS.txt`, and `latest.json` in the GitHub Release. Preliminary versions are published as prereleases and do not replace the stable `releases/latest` channel.

Legio checks for updates at startup and from Settings. The manifest distinguishes the installed package format. Tauri verifies the signature before installation. On Ubuntu and Fedora, package installation may require administrator authorization. On Windows, the NSIS installer is launched. Legio restarts after installation.

The Arch package `legio-launcher-bin` uses the AppImage published on GitHub. The pipeline updates the PKGBUILD and `.SRCINFO` on AUR for every stable release. AUR users update through their package manager; Legio displays the new version without overwriting files managed by pacman.

The AUR package declares `umu-launcher` as a runtime dependency, so AUR helpers install it alongside Legio. Arch users need the `multilib` repository enabled. Standalone AppImage, `.deb` and `.rpm` downloads do not bundle umu; install it separately to launch manually imported games through Proton or GE-Proton.

## Credentials

- `TAURI_SIGNING_PRIVATE_KEY` contains the private Tauri key used to sign installers. The public key is in `src-tauri/tauri.conf.json`. Keep a secure private backup: losing it prevents updates to existing installations.
- `AUR_SSH_PRIVATE_KEY` contains a dedicated SSH key. Its public key must be added to the AUR profile that publishes `legio-launcher-bin`.

Both keys are stored in the repository's GitHub secrets. The local Tauri key copy is at `~/.config/legio-release/updater.key`; the AUR key is at `~/.ssh/AUR_Github`. They must not be committed.
