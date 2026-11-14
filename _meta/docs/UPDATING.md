# Releases and updates

Increasing the version in `src-tauri/Cargo.toml` on `main` triggers a release after CI passes. CI calls the release workflow once; no second full CI run or redundant smoke build is started. The pipeline builds and signs Ubuntu (`.deb`), Fedora (`.rpm`), Linux (`.AppImage`), and Windows (`.exe`) packages, then publishes their `.sig` files, `SHA256SUMS.txt`, and `latest.json` in the GitHub Release. Preliminary versions are published as prereleases and do not replace the stable `releases/latest` channel.

Legio checks for updates at startup and from Settings. The manifest distinguishes the installed package format. Tauri verifies the signature before installation. On Ubuntu and Fedora, package installation may require administrator authorization. On Windows, the NSIS installer is launched. Legio restarts after installation.

The Arch package `legio-launcher-bin` uses the AppImage published on GitHub. The pipeline updates the PKGBUILD and `.SRCINFO` on AUR for every stable release. AUR users update through their package manager; Legio displays the new version without overwriting files managed by pacman.

The AUR package declares `umu-launcher` as a runtime dependency, so AUR helpers install it alongside Legio. Arch users need the `multilib` repository enabled. Standalone AppImage, `.deb` and `.rpm` downloads do not bundle umu; install it separately to launch manually imported games through Proton or GE-Proton.

## AUR development package

`legio-launcher-git` compiles current `main` sources on the user's system. It is separate from the stable binary package and does not need a Nightly release or an AUR commit for each source change. The [AUR workflow](../../.github/workflows/aur-git.yml) builds and publishes recipe changes, generating `.SRCINFO` with `makepkg`. See [package maintenance](../../packaging/aur-git/README.md).

The two packages conflict and share the installed profile. Back up data before switching from development code to an older stable version. Both must be updated through the package manager.

## Release protection

The release workflow runs a dependency audit (`audit.yml`: cargo-deny advisories, bans and sources, plus `pnpm audit --prod`) before building. Build and AUR jobs run in the GitHub environment `release`, so required reviewers gate every use of the signing secrets. The publish job attaches CycloneDX SBOMs (`legio-rust.cdx.json`, `legio-npm.cdx.json`) to the release and creates GitHub build provenance attestations for every release file.

## Repository setup (manual)

These steps cannot be done from the repository and must be completed by a maintainer before the next release:

1. Create the environment: Settings, Environments, New environment, named `release`. Enable Required reviewers and add the maintainers. Optionally restrict deployment branches to `main`.
2. Add these secrets to the `release` environment (not as repository secrets, so they are only exposed after approval):
   - `TAURI_SIGNING_PRIVATE_KEY`: the updater private key.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password. The release fails early when the key or the password is empty.
   - `AUR_SSH_PRIVATE_KEY`: the dedicated AUR SSH key.
   - `WINDOWS_CERTIFICATE` and `WINDOWS_CERTIFICATE_PASSWORD` (optional): a base64 encoded `.pfx` code signing certificate and its password. When absent, the build logs a notice and the Windows installer is published without Authenticode signing. The certificate is imported into the runner store and Tauri signs through `certificateThumbprint`.
3. Keep the repository variable `AUR_KNOWN_HOSTS` set to verified AUR host keys.
4. Remove any repository level copies of these secrets once the environment holds them.

### Password protected updater key

The updater key must be protected with a password. If the current key has none, generate a new one with `pnpm tauri signer generate` and choose a password. This changes the public key, which has two consequences:

- Replace `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` in the same change that enables the new secrets.
- Installations that carry the old public key cannot verify updates signed with the new key. They must reinstall manually from a release once. Plan the rotation as a deliberate release and announce it. A key that was generated without a password cannot be given one afterwards, so rotation is the only way to obtain a protected key.

Never rotate or commit the key as part of routine work.

### Verifying a download

Every release file is attested. With the GitHub CLI:

```sh
gh attestation verify Legio_<version>_amd64.AppImage --repo fraa2a/Legio
```

Checksums are in `SHA256SUMS.txt` in the same release:

```sh
sha256sum --check --ignore-missing SHA256SUMS.txt
```

On Windows, `Get-FileHash -Algorithm SHA256 <file>` prints the hash to compare.

## Credentials

- `TAURI_SIGNING_PRIVATE_KEY` contains the private Tauri key used to sign installers, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` its password. The public key is in `src-tauri/tauri.conf.json`. Keep a secure private backup of both: losing the key prevents updates to existing installations.
- `AUR_SSH_PRIVATE_KEY` contains a dedicated SSH key. Its public key must be added to the AUR profile that publishes `legio-launcher-bin`.
- `WINDOWS_CERTIFICATE` and `WINDOWS_CERTIFICATE_PASSWORD` are optional Authenticode credentials.

Configure these values as secrets of the `release` environment. Keep encrypted backups under the maintainers' credential-management policy. Local key locations are private operator configuration, not repository conventions. Never commit private keys or their backups.
