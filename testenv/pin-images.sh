#!/bin/sh
# Resolve the mutable tags below to immutable digests and rewrite testenv/.env.
# Run deliberately; review the diff; a digest change is an environment change.
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
set -eu
cd "$(dirname "$0")"

command -v docker >/dev/null || { echo "pin-images: docker is required" >&2; exit 1; }
docker buildx version >/dev/null 2>&1 || {
  echo "pin-images: docker buildx is required (it resolves digests without pulling)" >&2
  exit 1
}

pin() {  # pin <VAR> <repo:tag>
  digest=$(docker buildx imagetools inspect "$2" --format '{{.Manifest.Digest}}') || {
    echo "pin-images: cannot resolve $2" >&2
    exit 1
  }
  # A tag that resolves to an empty or non-sha256 string would otherwise be
  # written into .env as `NAME=repo@`, which compose accepts at `config` time
  # and only fails on at `up`. Refuse it here instead.
  case "$digest" in
    sha256:*) ;;
    *) echo "pin-images: $2 resolved to '$digest', which is not a digest" >&2; exit 1 ;;
  esac
  printf '# %s\n%s=%s@%s\n' "$2" "$1" "${2%%:*}" "$digest"
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
  echo '# jira-software ~700 MB; confluence ~800 MB.'
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
  pin TEAMCITY_IMAGE   jetbrains/teamcity-server:latest
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
  pin JIRA_IMAGE       atlassian/jira-software:10.3.24
  pin CONFLUENCE_IMAGE atlassian/confluence:9.2.21
} > "$tmp"

mv "$tmp" .env
echo "pin-images: wrote $(pwd)/.env"
