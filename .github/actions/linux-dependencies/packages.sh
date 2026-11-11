#!/usr/bin/env bash
# Installs the Linux build dependencies with bounded network steps.
# Usage: packages.sh plan|install
set -euo pipefail

packages=(
  build-essential
  curl
  file
  libayatana-appindicator3-dev
  libarchive-dev
  librsvg2-dev
  libssl-dev
  libwebkit2gtk-4.1-dev
  libxdo-dev
  wget
)
read -r -a extra_packages <<< "${EXTRA_PACKAGES:-}"
packages+=("${extra_packages[@]}")

archives=/var/cache/apt/archives
cache="$HOME/.cache/linux-packages"
plan_file="$RUNNER_TEMP/linux-packages.txt"

# Runs a command as root up to three times, killing each attempt after the given number of seconds.
attempt() {
  local seconds=$1
  shift
  local try
  for try in 1 2 3; do
    if sudo timeout --kill-after=10 "$seconds" "$@"; then
      return 0
    fi
    echo "::warning::Attempt $try of 3 failed or timed out: $*"
    if [[ $try -lt 3 ]]; then
      sleep $((try * 10))
    fi
  done
  echo "::error::Giving up after 3 attempts: $*"
  return 1
}

plan() {
  # Without --error-on=any a failed refresh only warns and apt installs from stale indexes.
  attempt 120 apt-get update --error-on=any

  # One line per archive to download: URL, file name, size and checksum.
  local uris
  uris=$(sudo apt-get install --yes --quiet=2 --print-uris "${packages[@]}")
  grep "^'" <<< "$uris" | sort > "$plan_file" || true

  local release architecture
  release=$(. /etc/os-release && echo "$ID-$VERSION_ID")
  architecture=$(dpkg --print-architecture)
  local prefix="linux-packages-$release-$architecture-"
  {
    echo "count=$(wc -l < "$plan_file")"
    echo "key=$prefix$(cut -d ' ' -f 2- "$plan_file" | sha256sum | cut -d ' ' -f 1)"
    echo "restore-key=$prefix"
  } >> "$GITHUB_OUTPUT"
}

install() {
  if compgen -G "$cache/*.deb" > /dev/null; then
    # apt checks the size and checksum of every archive against the signed index before using it.
    sudo cp "$cache"/*.deb "$archives/"
  fi

  # Completed archives are kept between attempts, so each attempt resumes the previous one.
  attempt 360 apt-get install --yes --download-only "${packages[@]}"

  # Keep only the archives this run needs, so the saved cache does not grow.
  rm -rf "$cache"
  mkdir -p "$cache"
  local name
  while read -r _ name _; do
    cp "$archives/$name" "$cache/"
  done < "$plan_file"

  sudo env DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-download "${packages[@]}"
}

case "${1:-}" in
  plan) plan ;;
  install) install ;;
  *)
    echo "Usage: packages.sh plan|install" >&2
    exit 2
    ;;
esac
