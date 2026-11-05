#!/usr/bin/env bash
# scripts/readme-diagrams.sh's own test, run by `just witness-unit`, against a stub
# `d2` so it needs no d2 installed. A render is asked of TALA by name. A d2 with no
# bundled TALA, a watermarked render and a failed render each replace nothing, so a
# README can never pick up another engine's layout or an unlicensed copy.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/readme-diagrams-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
failures=0
fail() { echo "FAIL: $1" >&2; failures=$((failures + 1)); }

tree=$scratch/tree
mkdir -p "$tree/scripts" "$tree/docs/diagrams"
cp "$here/readme-diagrams.sh" "$tree/scripts/"
git -C "$tree" init -q -b main
printf 'a -> b\n' >"$tree/docs/diagrams/one.d2"
printf 'c -> d\n' >"$tree/docs/diagrams/two.d2"

# The stub's behaviour comes from the environment: STUB_ENGINES is what `d2 layout`
# lists, STUB_BODY what a render writes, STUB_FAIL_ON a source name that fails.
stub=$scratch/d2
cat >"$stub" <<'SH'
#!/bin/sh
case $1 in
    layout) printf '%s\n' "$STUB_ENGINES"; exit 0 ;;
    --version) echo v0.0.0-stub; exit 0 ;;
esac
echo "$*" >>"$STUB_LOG"
for last; do :; done
for source; do case $source in *.d2) break ;; esac; done
case $source in *"$STUB_FAIL_ON"*) [ -n "$STUB_FAIL_ON" ] && { echo "stub failed" >&2; exit 1; } ;; esac
printf '%s' "$STUB_BODY" >"$last"
SH
chmod +x "$stub"
export READMES_D2=$stub STUB_LOG=$scratch/log
run() { # run <engines> <body> <fail on>
    STUB_ENGINES=$1 STUB_BODY=$2 STUB_FAIL_ON=$3 "$tree/scripts/readme-diagrams.sh" 2>&1
}
keeps_old() { # keeps_old <label>
    [ "$(cat "$tree/docs/diagrams/one-light.svg")" = old ] || fail "$1 replaced a picture"
}
bundled='tala (bundled) - the engine'

printf old >"$tree/docs/diagrams/one-light.svg"
if said=$(run 'dagre (bundled) - another' '<svg/>' ''); then
    fail "a d2 with no bundled TALA passed"
else
    case $said in *"no bundled TALA"*"v0.9.0"*) ;; *) fail "a d2 with no TALA said: $said" ;; esac
fi
keeps_old "a d2 with no TALA"

if said=$(run "$bundled" '<svg>UNLICENSED COPY</svg>' ''); then
    fail "a watermarked render passed"
else
    case $said in *"watermarked"*) ;; *) fail "a watermark said: $said" ;; esac
fi
keeps_old "a watermark"

# The second source fails after the first has rendered: still nothing is replaced.
if said=$(run "$bundled" '<svg/>' 'two.d2'); then
    fail "a failed render passed"
else
    case $said in *"two (light) failed"*"stub failed"*) ;; *) fail "a failed render said: $said" ;; esac
fi
keeps_old "a failed render"

: >"$scratch/log"
said=$(run "$bundled" '<svg>new</svg>' '') || fail "a good render failed: $said"
for file in one-light one-dark two-light two-dark; do
    [ "$(cat "$tree/docs/diagrams/$file.svg" 2>/dev/null)" = '<svg>new</svg>' ] || fail "$file.svg was not written"
done
[ "$(wc -l <"$scratch/log")" -eq 4 ] || fail "four renders were not asked for: $(cat "$scratch/log")"
if grep -v -- '--layout tala' "$scratch/log" | grep -q .; then fail "a render did not ask for TALA: $(cat "$scratch/log")"; fi

if [ "$failures" -gt 0 ]; then echo "readme-diagrams-test: $failures failure(s)" >&2; exit 1; fi
echo "readme-diagrams-test: ok"
