#!/bin/sh
# Resolve the mutable tags below to immutable digests and rewrite testenv/.env.
# Run deliberately; review the diff; a digest change is an environment change.
#
#   ./pin-images.sh                    # rewrite .env
#   ./pin-images.sh --move VAR         # take a guarded pin's new digest (see below)
#   ./pin-images.sh --out PATH         # write somewhere else, e.g. to compare
#
# Digests come from `docker buildx imagetools inspect`, which reads the
# registry's manifest and downloads nothing else. The plan specified
# `docker pull` + `.RepoDigests`; that was changed because pinning all six tags
# by pulling costs roughly 13 GB -- ~10 GB of it the TeamCity server image that
# the whole point of `profiles:` is to keep off the disk. The two methods were
# verified to agree: gitea/gitea:1 resolved to
# sha256:d20286ca2b2e170fdf628e7231b8a31a3220ade39ff462b55041d43d1fc757dd both
# by imagetools inspect and by pull + `docker image inspect .RepoDigests`.
# They agree because for a multi-arch tag both report the digest of the
# manifest *index*, not of one platform's manifest.
#
# GUARDED PINS. Three of the images are the ones a seed script walks a
# first-start wizard on, and that seed REFUSES any digest but the one its form
# fields were read off (`VERIFIED_*_IMAGE` in `seed-atlassian.sh` and
# `seed-teamcity.sh`). A plain re-pin used to move those with the tag: on
# 2026-09-02 `atlassian/confluence:9.2.21` had been retagged upstream, and the
# re-pin for #264 wrote a Confluence digest the Atlassian seed would not touch
# (#268). So for those three the script writes the digest the seed guards on
# and only REPORTS when the tag now resolves elsewhere; `--move VAR` is the
# explicit way to take the new digest, after which the guarding seed is owed a
# re-derivation against the new image. The guard values are read out of the
# seed scripts, not copied here, so the two cannot disagree.
set -eu

usage() {
  echo "usage: ./pin-images.sh [--move VAR]... [--out PATH]"
  echo "  --move VAR   write VAR's freshly resolved digest even though a seed guards on"
  echo "               another one (VAR is one of the guarded pins; repeatable)"
  echo "  --out PATH   write PATH instead of testenv/.env"
}

out=
move=
while [ $# -gt 0 ]; do
  case "$1" in
    --move) [ $# -ge 2 ] || { usage >&2; exit 2; }; move="$move $2"; shift 2 ;;
    --out)  [ $# -ge 2 ] || { usage >&2; exit 2; }; out=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "pin-images: unknown argument '$1'" >&2; usage >&2; exit 2 ;;
  esac
done
# A relative --out is relative to where the caller stands, so it is resolved
# before the cd; the default is testenv/.env itself, so that one is resolved
# after it.
case "$out" in ''|/*) ;; *) out="$PWD/$out" ;; esac

cd "$(dirname "$0")"
[ -n "$out" ] || out="$PWD/.env"

command -v docker >/dev/null || { echo "pin-images: docker is required" >&2; exit 1; }
docker buildx version >/dev/null 2>&1 || {
  echo "pin-images: docker buildx is required (it resolves digests without pulling)" >&2
  exit 1
}

resolve() {  # resolve <repo:tag>  -> the tag's current digest on stdout
  _d=$(docker buildx imagetools inspect "$1" --format '{{.Manifest.Digest}}') || {
    echo "pin-images: cannot resolve $1" >&2
    return 1
  }
  # A tag that resolves to an empty or non-sha256 string would otherwise be
  # written into .env as `NAME=repo@`, which compose accepts at `config` time
  # and only fails on at `up`. Refuse it here instead.
  case "$_d" in
    sha256:*) printf '%s\n' "$_d" ;;
    *) echo "pin-images: $1 resolved to '$_d', which is not a digest" >&2; return 1 ;;
  esac
}

line() {  # line <VAR> <repo:tag> <digest>  -> the two .env lines for one image
  printf '# %s\n%s=%s@%s\n' "$2" "$1" "${2%%:*}" "$3"
}

pin() {  # pin <VAR> <repo:tag>  -- unguarded: whatever the tag resolves to now
  digest=$(resolve "$2") || exit 1
  line "$1" "$2" "$digest"
}

# The digest a seed script guards on, read from the seed itself. The line
# shape both seeds use is `VERIFIED_<PRODUCT>_IMAGE=sha256:<64 hex>` at column
# 0, exactly one per product; anything else -- the line renamed, gone,
# duplicated, or the digest malformed -- is a stop, because a guard this
# script cannot read is a guard it would silently pin past.
guarded_digest() {  # guarded_digest <seed script> <PRODUCT>  -> digest on stdout
  [ -f "$1" ] || { echo "pin-images: no seed script $1 beside this script" >&2; return 1; }
  _g=$(sed -n "s/^VERIFIED_$2_IMAGE=\(sha256:[0-9a-f]\{64\}\)$/\1/p" "$1")
  # 7 for `sha256:` + 64 hex: one match exactly. Two matches would be 143 long.
  [ "${#_g}" -eq 71 ] || {
    echo "pin-images: cannot read VERIFIED_$2_IMAGE=sha256:<64 hex> from $1" >&2
    echo "  (found: '${_g:-nothing}'). The guard moved; move this script's pattern with it." >&2
    return 1
  }
  printf '%s\n' "$_g"
}

moving() {  # moving <VAR>  -> true when --move VAR was given
  case " $move " in *" $1 "*) return 0 ;; esac
  return 1
}

held=0
guarded_vars=
# pin_guarded <VAR> <repo:tag> <seed script> [note printed on --move]
#
# Writes the seed's guarded digest. If the tag has moved on, says so with both
# digests -- to stderr, because stdout inside the block below IS the .env
# file. With `--move VAR` the fresh digest is written instead and the owed
# re-derivation is named. The seed's variable is VERIFIED_<VAR minus _IMAGE>_IMAGE:
# CONFLUENCE_IMAGE here is VERIFIED_CONFLUENCE_IMAGE there.
pin_guarded() {
  guarded_vars="$guarded_vars $1"
  product=${1%_IMAGE}
  fresh=$(resolve "$2") || exit 1
  guarded=$(guarded_digest "$3" "$product") || exit 1
  if moving "$1"; then
    _rest=
    for _m in $move; do [ "$_m" = "$1" ] || _rest="$_rest $_m"; done
    move=$_rest
    if [ "$fresh" = "$guarded" ]; then
      echo "pin-images: $1: --move given, but $2 still resolves to the digest $3 guards on; nothing moved" >&2
    else
      echo "pin-images: MOVED $1 off the digest $3 guards on." >&2
      echo "  written:  $fresh" >&2
      echo "  guarded:  $guarded  (VERIFIED_${product}_IMAGE)" >&2
      echo "  $3 will refuse the new container until its wizard walk is re-derived" >&2
      echo "  against the new image and VERIFIED_${product}_IMAGE is set to the written digest." >&2
      [ -z "${4:-}" ] || echo "  $4" >&2
    fi
    line "$1" "$2" "$fresh"
  else
    if [ "$fresh" != "$guarded" ]; then
      held=$((held + 1))
      echo "pin-images: HOLDING $1 at the digest $3 guards on." >&2
      echo "  $2 now resolves to: $fresh" >&2
      echo "  $3 guards on:       $guarded  (VERIFIED_${product}_IMAGE)" >&2
      echo "  Pinning the new digest would make the seed refuse its own container; its" >&2
      echo "  wizard walk has to be re-derived first. To take it anyway:" >&2
      echo "    ./pin-images.sh --move $1" >&2
    fi
    line "$1" "$2" "$guarded"
  fi
}

# WRITE TO A TEMPORARY FILE, MOVE IT INTO PLACE ONLY ON SUCCESS. `> .env`
# truncates the destination the instant the block starts, and every `pin` in it
# is a network round trip that can fail -- a Docker Hub 429 mid-run (observed
# 2026-08-31) therefore left a .env holding the header comment and none of the
# seventeen digests, which `docker compose` reports as a missing-variable error
# somewhere else entirely. Same reasoning as `just inventory`, which learned it
# the same way.
#
tmp=$(mktemp "${TMPDIR:-/tmp}/knobas-env.XXXXXX")
trap 'rm -f "$tmp"' EXIT

# The heredoc-style block below writes .env. Its comment lines contain literal
# backticks and $-free prose that must reach the file verbatim, so single
# quotes are correct and SC2016's suggestion would break them.
#
# The directive has to sit immediately above the `{` it applies to -- putting
# anything between them silently re-points it at that line instead, which the
# `testenv` workflow's shellcheck would catch if it were enabled.
# shellcheck disable=SC2016
{
  echo '# Image digest pins for testenv/docker-compose.yml. Public digests only --'
  echo '# no secrets live here. Regenerate with ./pin-images.sh, review the diff.'
  echo '#'
  echo '# Approximate pull cost, compressed, amd64 (why the big ones are opt-in'
  echo '# profiles): gitea ~110 MB, uptime-kuma ~250 MB, node:22-alpine ~60 MB --'
  echo '# the default `docker compose up` set, well under 1 GB. teamcity-server'
  echo '# ~2.5 GB compressed and ~10 GB on disk once its data directory exists;'
  echo '# teamcity-agent ~1 GB compressed (its own JDK and toolchain), behind the'
  echo '# same profile; jira-software ~700 MB; confluence ~800 MB.'
  echo '#'
  echo '# The two build stages of mockd.Dockerfile are pinned for the same reason'
  echo '# the services are: an unpinned `rust:1-slim` makes the mockd container'
  echo '# unreproducible. rust-toolchain.toml still decides the compiler --'
  echo '# rustup inside the image obeys it -- so this pins the base OS, not rustc.'
  pin RUST_IMAGE       rust:1-slim
  pin RUNTIME_IMAGE    debian:trixie-slim
  echo '#'
  pin GITEA_IMAGE      gitea/gitea:1
  pin KUMA_IMAGE       louislam/uptime-kuma:2
  pin NODE_IMAGE       node:22-alpine
  # Guarded: `seed-teamcity.sh` drives the server's first-start wizard, whose
  # endpoints were read off this digest. `latest` left it on 2026-09-02
  # (2026.1.3 -> 2026.2), so a plain run holds and reports (#268).
  pin_guarded TEAMCITY_IMAGE jetbrains/teamcity-server:latest seed-teamcity.sh \
    'TEAMCITY_AGENT_IMAGE below is pinned by version tag to stay on the server line; bump that tag too.'
  # The build agent beside it (#264), unguarded: it registers through the
  # supported `SERVER_URL` protocol, no wizard. But it must not be AHEAD of the
  # server -- an agent newer than the server is refused at registration -- and
  # `latest` is a 2026.2 agent while the guarded server is 2026.1.3. So the
  # agent's tag is the server's version, spelled out, and moves with it: after
  # `--move TEAMCITY_IMAGE` and the re-derivation it names, set this tag to
  # the new server's version in the same change.
  pin TEAMCITY_AGENT_IMAGE jetbrains/teamcity-agent:2026.1.3
  # These two are pinned to what a **timebomb licence actually starts**, not
  # to the newest release (#49, revised 2026-08-31). The distinction is the
  # whole reason they are here: an image that cannot be licensed cannot be
  # booted, and an unbootable container checks nothing.
  #
  # The only free door left. Atlassian stopped self-service DC trials on
  # 2026-03-30 ("From March 30, 2026, you won't be able to generate trial
  # licences for Atlassian-owned Data Center products") and stopped selling DC
  # to new customers the same day, so the published **timebomb** keys -- 10
  # user, valid 3 hours from when applied, no my.atlassian.com account -- are
  # what these containers run on:
  #   developer.atlassian.com/platform/marketplace/timebomb-licenses-for-testing-server-apps/
  #
  # Why these versions and not the newest:
  #   * Jira 11.x is reported to REJECT the timebomb key outright ("This
  #     license is invalid", Jira Software DC 11.0.0, developer community
  #     thread 102402, 2026-08-31; Atlassian's reply offered an ECOHELP
  #     vendor-only route rather than a fix). We were briefly pinned to
  #     11.3.10 and moved off it on that evidence.
  #   * 10.3 and 9.2 are the LTS lines that `sooperset/mcp-atlassian`'s e2e
  #     Docker harness runs its timebomb licences against (`JIRA_VERSION`
  #     defaults to 10.3-jdk17, `CONFLUENCE_VERSION` to 9.2-jdk17), with
  #     confluence:9.2.21 verified explicitly on 2026-06-18.
  #   * atlassian/jira-software:10.3.24, :10.3, :10.3-jdk17 and :10.3.24-jdk17
  #     are one and the same digest, so the explicit patch costs nothing.
  #
  # Side benefit: 10.3 is two majors CLOSER to `specs/jira-dc-rest.wadl` (Jira
  # 9.17.0) than 11.3 was. It still is not aligned, and cannot be -- Atlassian
  # publishes no WADL past 9.17.x (9.18.0 and every 10.x/11.x probed answer
  # 404; `.../REST/latest/` redirects to 9.17.0). The container being newer
  # than its contract document is the gap `real-atlassian` exists to measure.
  # One image for both Atlassian databases. Jira 10 removed embedded H2, so
  # this is not optional scaffolding -- see docker-compose.yml.
  pin ATLASSIAN_DB_IMAGE postgres:15-alpine
  # Guarded, both: `seed-atlassian.sh` walks each product's wizard. Even a
  # patch tag moves -- confluence:9.2.21 was retagged upstream between
  # 2026-08-31 and 2026-09-02 -- and the seed refuses the retag.
  pin_guarded JIRA_IMAGE       atlassian/jira-software:10.3.24 seed-atlassian.sh
  pin_guarded CONFLUENCE_IMAGE atlassian/confluence:9.2.21     seed-atlassian.sh
} > "$tmp"

# A --move that named no guarded pin is a typo, and a typo must not pass as a
# successful run: nothing has been moved into place yet, so stop here.
if [ -n "$move" ]; then
  echo "pin-images: --move$move: not a guarded pin (guarded:$guarded_vars)." >&2
  echo "  Unguarded pins always resolve fresh; there is nothing to move." >&2
  exit 2
fi

mv "$tmp" "$out"
echo "pin-images: wrote $out"
# Drift is information, not an error: the file just written is the right one
# for the seeds as they are, and an exit 1 here would fail every routine re-pin
# from the day upstream retags until someone re-derives a wizard walk. The
# count goes where the detail went, so `2>/dev/null` hides both or neither.
[ "$held" -eq 0 ] || echo "pin-images: $held guarded pin(s) held at the seed's digest; see above for --move." >&2
