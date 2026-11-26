#!/usr/bin/env bash
# Fail unless every bundle `tauri build` made on this machine carries the
# pinned PostgreSQL archive (docs/decisions/2026-11-14-knobas-ships-its-postgresql.md).
#
# A bundle without it still installs and still runs: it downloads PostgreSQL
# on a machine's first start, which is the 15 s and the need for a network the
# archive is there to remove. Nothing else would notice it missing, so
# `release.yml` runs this between `tauri build` and the upload.
#
# Each bundle is opened with its own format's tool and the file inside is
# hashed against `crates/knobas-app/postgresql/archives.sha256`:
#   macOS  the `.app`, and the `.app` inside the mounted `.dmg`
#   Linux  the `.deb` (dpkg-deb), the `.rpm` (rpm2cpio) and the `.AppImage`
#          (its own `--appimage-extract`); a package's copy must also be
#          readable by every user, since root owns it once installed, and
#          must start with nothing but glibc from the host
#          (`scripts/check-postgres-starts.sh`; 18.6.0 needed the host's
#          libxml2.so.2, which Ubuntu 26.04 and Fedora 45 no longer ship).
#          The `.rpm` must also require `libssl.so.3()(64bit)`, which
#          knobas-app links: Fedora 45's own OpenSSL is 4, and only that
#          requirement brings in its `openssl3-libs`.
# Windows is `scripts/check-bundled-postgres.ps1`: `msiexec` and 7-Zip are at
# home in PowerShell, and a `/a` put through Git Bash arrives as a path.
#
# Usage: scripts/check-bundled-postgres.sh [bundle-dir]
#   bundle-dir  default: target/release/bundle
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
bundles=${1:-$root/target/release/bundle}
pins="$root/crates/knobas-app/postgresql/archives.sha256"

case "$(uname -s)" in
  Darwin) target=aarch64-apple-darwin ;;
  Linux)  target=x86_64-unknown-linux-gnu ;;
  *) echo "error: no check for $(uname -s) here; Windows is scripts/check-bundled-postgres.ps1" >&2; exit 2 ;;
esac

line=$(grep -- "-$target\.tar\.gz\$" "$pins" || true)
if [ -z "$line" ] || [ "$(printf '%s\n' "$line" | wc -l)" -ne 1 ]; then
  echo "error: $pins pins no one archive for $target" >&2
  exit 2
fi
want=${line%% *}
asset=${line##* }
version=${asset#postgresql-}
version=${version%-"$target".tar.gz}
name="postgresql-$version.tar.gz"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | cut -d' ' -f1
  else
    shasum -a 256 | cut -d' ' -f1
  fi
}

scratch=$(mktemp -d)
mounted=""
cleanup() {
  [ -z "$mounted" ] || hdiutil detach -quiet "$mounted" >/dev/null 2>&1 || true
  rm -rf "$scratch"
}
trap cleanup EXIT

checked=0
failed=0
# verdict <bundle> <sha256 of the archive in it, or empty> [ls-style mode of it]
# A package's copy is root's once installed, so every user must be able to
# read it: the mode is given for a `.deb` and an `.rpm`.
verdict() {
  local mode=${3:-}
  checked=$((checked + 1))
  if [ -z "$2" ]; then
    echo "FAIL $1: no postgresql/$name in it"
  elif [ "$2" != "$want" ]; then
    echo "FAIL $1: its postgresql/$name has sha256 $2, and $asset is pinned at $want"
  elif [ -n "$mode" ] && [[ $mode != ?r??r??r??* ]]; then
    echo "FAIL $1: its postgresql/$name is $mode, which not every user can read"
  else
    echo "ok   $1: carries $asset"
    return 0
  fi
  failed=$((failed + 1))
}
# of_file <path>: the file's sha256, empty when it is not there.
of_file() {
  if [ -f "$1" ]; then sha256 < "$1"; fi
}
# starts <bundle> <the archive taken out of it>: the PostgreSQL in it starts
# on this machine with only glibc from the host. Run once the hash matched.
starts() {
  checked=$((checked + 1))
  if "$root/scripts/check-postgres-starts.sh" "$2"; then
    echo "ok   $1: its PostgreSQL starts"
  else
    echo "FAIL $1: its PostgreSQL does not start on its own libraries"
    failed=$((failed + 1))
  fi
}

shopt -s nullglob
case "$target" in
  aarch64-apple-darwin)
    for app in "$bundles"/macos/*.app; do
      verdict "$app" "$(of_file "$app/Contents/Resources/postgresql/$name")"
    done
    for dmg in "$bundles"/dmg/*.dmg; do
      mounted="$scratch/dmg"
      mkdir -p "$mounted"
      hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mounted" "$dmg"
      inside=""
      for app in "$mounted"/*.app; do
        inside=$(of_file "$app/Contents/Resources/postgresql/$name")
      done
      hdiutil detach -quiet "$mounted"
      mounted=""
      verdict "$dmg" "$inside"
    done
    ;;
  x86_64-unknown-linux-gnu)
    for deb in "$bundles"/deb/*.deb; do
      # `-rw-r--r-- root/root 12184736 2026-11-14 12:00 ./usr/lib/knobas/postgresql/<name>`
      entry=$(dpkg-deb --fsys-tarfile "$deb" | tar -tvf - | grep -- "/postgresql/$name\$" || true)
      inside=""
      if [ -n "$entry" ]; then
        member=${entry##* }
        dpkg-deb --fsys-tarfile "$deb" | tar -xOf - "$member" > "$scratch/deb-$name"
        inside=$(of_file "$scratch/deb-$name")
      fi
      verdict "$deb" "$inside" "${entry%% *}" && starts "$deb" "$scratch/deb-$name"
      rm -f "$scratch/deb-$name"
    done
    for rpm in "$bundles"/rpm/*.rpm; do
      entry=$(rpm -qlvp "$rpm" 2>/dev/null | grep -- "/postgresql/$name\$" || true)
      inside=""
      if [ -n "$entry" ]; then
        mkdir -p "$scratch/rpm"
        (cd "$scratch/rpm" && rpm2cpio "$rpm" | cpio --quiet -idm "*/postgresql/$name")
        found=$(find "$scratch/rpm" -type f -name "$name" | sed -n 1p)
        inside=$(of_file "$found")
        if verdict "$rpm" "$inside" "${entry%% *}"; then starts "$rpm" "$found"; fi
        checked=$((checked + 1))
        if rpm -qpR "$rpm" 2>/dev/null | grep -qxF 'libssl.so.3()(64bit)'; then
          echo "ok   $rpm: requires libssl.so.3"
        else
          echo "FAIL $rpm: does not require libssl.so.3()(64bit) (tauri.conf.json bundle.linux.rpm.depends)"
          failed=$((failed + 1))
        fi
        rm -rf "$scratch/rpm"
      else
        verdict "$rpm" "" "${entry%% *}"
      fi
    done
    for appimage in "$bundles"/appimage/*.AppImage; do
      mkdir -p "$scratch/appimage"
      chmod +x "$appimage"
      # Unpacks into `squashfs-root/` where it is run; needs no FUSE.
      (cd "$scratch/appimage" && "$appimage" --appimage-extract "usr/lib/*/postgresql/$name" >/dev/null 2>&1) || true
      found=$(find "$scratch/appimage" -type f -name "$name" | sed -n 1p)
      inside=$(of_file "$found")
      if verdict "$appimage" "$inside"; then starts "$appimage" "$found"; fi
      rm -rf "$scratch/appimage"
    done
    ;;
esac

if [ "$checked" -eq 0 ]; then
  echo "error: no bundle under $bundles to check" >&2
  exit 1
fi
if [ "$failed" -ne 0 ]; then
  echo "error: $failed of the checks above failed. Run scripts/fetch-postgres.sh before tauri build." >&2
  exit 1
fi
echo "check-bundled-postgres: $checked checks passed: every bundle carries $asset"
