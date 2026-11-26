#!/usr/bin/env bash
# One `.sig` beside every update artifact this runner built (ADR-0052, decisions D15), by the Tauri CLI's signer with
# the repository's `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), then each checked by the
# verifier knobas itself uses (`knobas-update-verify`), against the two public keys committed in
# `crates/knobas-update/keys/`. The build must trust exactly those two keys, by id, or this fails before upload.
#
# The artifacts are the upload step's own globs plus the Mac's `.app.tar.gz`. Relative paths only, for the reason
# release.yml's `tauri build` step gives (a Windows path through a bash string loses its backslashes).
#
#     scripts/sign-update-artifacts.sh                 # from the repository's root, after `tauri build`
set -euo pipefail
shopt -s nullglob

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    echo "::error::the repository has no TAURI_SIGNING_PRIVATE_KEY secret (docs/development.md, Update signing)" >&2
    exit 1
fi

os=${RUNNER_OS:-$(uname -s)}
case "$os" in
    macOS|Darwin) files=(target/release/bundle/dmg/*.dmg target/release/bundle/macos/*.app.tar.gz) ;;
    Windows|MINGW*|MSYS*) files=(target/release/bundle/nsis/*.exe target/release/bundle/msi/*.msi) ;;
    Linux) files=(target/release/bundle/deb/*.deb target/release/bundle/rpm/*.rpm target/release/bundle/appimage/*.AppImage) ;;
    *) echo "::error::no artifact patterns for $os" >&2; exit 1 ;;
esac
if [ ${#files[@]} -eq 0 ]; then
    echo "::error::no update artifact to sign for $os" >&2
    exit 1
fi

for key in crates/knobas-update/keys/primary.pub crates/knobas-update/keys/spare.pub; do
    [ -f "$key" ] || { echo "::error::$key is not committed: a build signed now would trust no key (docs/development.md, Update signing)" >&2; exit 1; }
done

if [ -z "${KNOBAS_UPDATE_SKIP_SIGN:-}" ]; then
    for file in "${files[@]}"; do
        rm -f "$file.sig"
        node app/node_modules/@tauri-apps/cli/tauri.js signer sign "$file"
        [ -s "$file.sig" ] || { echo "::error::the signer wrote no $file.sig" >&2; exit 1; }
    done
fi

# The release run's build of the verifier fails on a compile-time extra key (decisions D16).
if [ -n "${KNOBAS_UPDATE_EXTRA_PUBKEY:-}" ]; then
    echo "::error::KNOBAS_UPDATE_EXTRA_PUBKEY is set: a release never trusts a third key" >&2
    exit 1
fi
verify=${KNOBAS_UPDATE_VERIFY:-}
if [ -z "$verify" ]; then
    cargo build --release --locked -p knobas-update --bin knobas-update-verify
    verify=target/release/knobas-update-verify
    [ -x "$verify" ] || verify=target/release/knobas-update-verify.exe
fi

want=$(for key in crates/knobas-update/keys/primary.pub crates/knobas-update/keys/spare.pub; do "$verify" --id "$key"; done | sort | tr '\n' ' ')
trusted=$("$verify" --keys | cut -f1 | tr -d '\r' | sort | tr '\n' ' ')
if [ "$want" != "$trusted" ]; then
    echo "::error::the build trusts the keys [$trusted], not the two committed ones [$want]" >&2
    exit 1
fi
echo "the build trusts exactly the committed keys: $want"

for file in "${files[@]}"; do
    "$verify" "$file" "$file.sig"
done
