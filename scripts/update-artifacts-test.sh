#!/usr/bin/env bash
# The update artifacts' scripts (ADR-0052, decisions D15), run by `just witness-unit`. No network, no cargo, no node.
#
# scripts/update-manifest.sh over fixture folders: all six artifacts; Windows only; none signed (exit 1, no manifest);
# a `.sig` with no asset; a draft's asset list, whose own URLs are `untagged-…` (the manifest names the tag's).
# scripts/pack-mac-update.sh over a fake bundle: with PostgreSQL's archive, and without (refused).
# scripts/sign-update-artifacts.sh: no secret (fails by name); no committed keys (fails); a build that trusts other
# keys than the committed ones (fails); the committed two (verifies each artifact).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/update-artifacts-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
failures=0
fail() { echo "FAIL: $1" >&2; failures=$((failures + 1)); }

# ---- update-manifest.sh ----------------------------------------------------
v=3.2.9-ux28
names=("knobas_${v}_x64-setup.exe" "knobas_${v}_x64_en-US.msi" "knobas_${v}_aarch64.app.tar.gz" "knobas_${v}_amd64.AppImage" "knobas_${v}_amd64.deb" "knobas-${v}-1.x86_64.rpm")
mig=$scratch/migrations
mkdir -p "$mig"
touch "$mig/0001_first.sql" "$mig/0099_a.sql" "$mig/0102_the_last.sql"
assets_json() {
    local url=$1; shift
    local items=() name
    for name in "$@"; do items+=("{\"name\":\"$name\",\"size\":${#name}00,\"url\":\"$url/$name\"}"); done
    local IFS=,
    printf '{"assets":[%s]}' "${items[*]}"
}
manifest() { MIGRATIONS=$mig REPO=BFoerschner/knobas PUB_DATE=2026-11-22T09:41:07Z "$here/update-manifest.sh" "v$v" "$1" "$2"; }

all=$scratch/all
mkdir -p "$all"
for name in "${names[@]}"; do printf 'sig of %s\n\n' "$name" >"$all/$name.sig"; done
assets_json "https://github.com/BFoerschner/knobas/releases/download/v$v" "${names[@]}" >"$scratch/all.json"
if out=$(manifest "$all" "$scratch/all.json"); then
    [ "$(jq -r .version <<<"$out")" = "$v" ] || fail "the version is $(jq -r .version <<<"$out")"
    [ "$(jq '.platforms | length' <<<"$out")" -eq 6 ] || fail "all six: $(jq -c '.platforms | keys' <<<"$out")"
    [ "$(jq .knobas.schema <<<"$out")" -eq 102 ] || fail "the schema is $(jq .knobas.schema <<<"$out"), not 102"
    [ "$(jq -r '.platforms["windows-x86_64-nsis"].url' <<<"$out")" = "https://github.com/BFoerschner/knobas/releases/download/v$v/knobas_${v}_x64-setup.exe" ] || fail "the NSIS url"
    [ "$(jq -r '.platforms["linux-x86_64-rpm"].signature' <<<"$out")" = "sig of knobas-${v}-1.x86_64.rpm" ] || fail "the rpm signature is the .sig's text, its trailing blank lines cut"
    [ "$(jq '.knobas.sizes["darwin-aarch64-app"]' <<<"$out")" -eq "${#names[2]}00" ] || fail "the Mac's size"
    [ "$(jq -r .pub_date <<<"$out")" = 2026-11-22T09:41:07Z ] || fail "the publish date"
else
    fail "six signed artifacts wrote no manifest"
fi

win=$scratch/win
mkdir -p "$win"
cp "$all/${names[0]}.sig" "$all/${names[1]}.sig" "$win/"
out=$(manifest "$win" "$scratch/all.json") || fail "Windows only wrote no manifest"
[ "$(jq -c '.platforms | keys' <<<"$out")" = '["windows-x86_64-msi","windows-x86_64-nsis"]' ] || fail "Windows only: $(jq -c '.platforms | keys' <<<"$out")"

# Mutant: upload latest.json when no artifact is signed.
none=$scratch/none
mkdir -p "$none"
if manifest "$none" "$scratch/all.json" >"$scratch/none.out" 2>/dev/null; then fail "no signed artifact still wrote a manifest"; fi
[ ! -s "$scratch/none.out" ] || fail "no signed artifact printed $(cat "$scratch/none.out")"

# A .sig whose artifact is not in the release: not offered.
assets_json "https://x" "${names[0]}" >"$scratch/one.json"
out=$(manifest "$all" "$scratch/one.json") || fail "one asset wrote no manifest"
[ "$(jq -c '.platforms | keys' <<<"$out")" = '["windows-x86_64-nsis"]' ] || fail "a .sig with no artifact was listed: $(jq -c '.platforms | keys' <<<"$out")"

# A draft's asset list: its URLs are untagged, the manifest's are the tag's.
assets_json "https://github.com/BFoerschner/knobas/releases/download/untagged-0d1e2f" "${names[@]}" >"$scratch/draft.json"
out=$(manifest "$all" "$scratch/draft.json") || fail "a draft's assets wrote no manifest"
if grep -q untagged <<<"$out"; then fail "a draft's untagged URL reached the manifest"; fi

# ---- pack-mac-update.sh ----------------------------------------------------
bundle=$scratch/bundle
mkdir -p "$bundle/knobas.app/Contents/MacOS" "$bundle/knobas.app/Contents/Resources/postgresql"
printf 'exe' >"$bundle/knobas.app/Contents/MacOS/knobas-app"
printf 'pg' >"$bundle/knobas.app/Contents/Resources/postgresql/postgresql-18.1.0-aarch64-apple-darwin.tar.gz"
if "$here/pack-mac-update.sh" "$v" "$bundle" >/dev/null; then
    tar -tzf "$bundle/knobas_${v}_aarch64.app.tar.gz" | grep -q '^knobas.app/Contents/MacOS/knobas-app$' || fail "the archive does not hold the executable"
else
    fail "a bundle with its PostgreSQL archive was not packed"
fi
rm "$bundle/knobas.app/Contents/Resources/postgresql/"*.tar.gz
if "$here/pack-mac-update.sh" "$v" "$bundle" >/dev/null 2>&1; then fail "a bundle without PostgreSQL's archive was packed"; fi

# ---- sign-update-artifacts.sh ----------------------------------------------
tree=$scratch/tree
mkdir -p "$tree/scripts" "$tree/target/release/bundle/deb" "$tree/target/release/bundle/appimage" "$tree/crates/knobas-update/keys"
cp "$here/sign-update-artifacts.sh" "$tree/scripts/"
printf 'deb' >"$tree/target/release/bundle/deb/knobas_${v}_amd64.deb"
printf 'appimage' >"$tree/target/release/bundle/appimage/knobas_${v}_amd64.AppImage"
: >"$tree/target/release/bundle/deb/knobas_${v}_amd64.deb.sig"
: >"$tree/target/release/bundle/appimage/knobas_${v}_amd64.AppImage.sig"
sign() { (cd "$tree" && RUNNER_OS=Linux KNOBAS_UPDATE_SKIP_SIGN=1 KNOBAS_UPDATE_VERIFY=$scratch/verify scripts/sign-update-artifacts.sh); }
# A fake verifier: the ids of two keys, which it trusts as FAKE_TRUSTS says, and every file verifies.
cat >"$scratch/verify" <<'EOF'
#!/usr/bin/env bash
case "$1" in
    --id) sed -n 's/^id: //p' "$2" ;;
    --keys) for id in $FAKE_TRUSTS; do printf '%s\tkey\n' "$id"; done ;;
    *) echo "verified $1"; echo "$1" >>"$FAKE_LOG" ;;
esac
EOF
chmod +x "$scratch/verify"
export FAKE_LOG=$scratch/verified.log

if err=$(TAURI_SIGNING_PRIVATE_KEY='' sign 2>&1); then fail "signing without the secret succeeded"; fi
grep -q "the repository has no TAURI_SIGNING_PRIVATE_KEY secret" <<<"$err" || fail "no secret, and the failure does not say so: $err"

if err=$(TAURI_SIGNING_PRIVATE_KEY=k sign 2>&1); then fail "signing with no committed keys succeeded"; fi
grep -q "primary.pub is not committed" <<<"$err" || fail "no keys, and the failure does not name them: $err"

printf 'id: AAAA\n' >"$tree/crates/knobas-update/keys/primary.pub"
printf 'id: BBBB\n' >"$tree/crates/knobas-update/keys/spare.pub"
if err=$(TAURI_SIGNING_PRIVATE_KEY=k FAKE_TRUSTS="AAAA CCCC" sign 2>&1); then fail "a build trusting a third key passed"; fi
grep -q "not the two committed ones" <<<"$err" || fail "a third key, and the failure does not say so: $err"

: >"$FAKE_LOG"
TAURI_SIGNING_PRIVATE_KEY=k FAKE_TRUSTS="BBBB AAAA" sign >/dev/null 2>&1 || fail "the committed two keys did not pass"
[ "$(wc -l <"$FAKE_LOG" | tr -d ' ')" -eq 2 ] || fail "every artifact is verified: $(cat "$FAKE_LOG")"

if [ "$failures" -gt 0 ]; then
    echo "update-artifacts-test: $failures failure(s)" >&2
    exit 1
fi
echo "update-artifacts-test: ok"
