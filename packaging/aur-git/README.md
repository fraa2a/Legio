# AUR development package

`legio-launcher-git` builds the latest `main` sources locally. It conflicts with the stable `legio-launcher-bin`; both use the same application profile. Back up your data before switching to an older version.

- Git sources use `SKIP` checksums because the branch moves.
- The local launcher wrapper has a fixed checksum. After editing it, run `updpkgsums` from `pacman-contrib`; the packaging test detects stale hashes.
- `pkgver()` reads Cargo's version and appends the Git revision count and commit.
- Licenses are installed in `/usr/share/licenses/legio-launcher-git`, the standard Arch location. `$pkgdir` is makepkg's staging directory, not the user's home.
- The wrapper marks the installation as package-managed, preventing the app updater from replacing pacman-owned files.

## Publish

The **AUR development package** workflow builds with `makepkg` on Arch, generates `.SRCINFO`, then pushes the recipe over SSH. The first push creates the AUR package base. It runs when the recipe changes on `main`, or manually through Actions; PR runs build but never publish.

The workflow uses `AUR_SSH_PRIVATE_KEY` and verified `AUR_KNOWN_HOSTS`. Review the selected ref before manually publishing. Do not update AUR for each application commit: users rebuild the VCS package through their AUR helper's development-package update option.
