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

# The heredoc-style block below writes .env. Its comment lines contain literal
# backticks and $-free prose that must reach the file verbatim, so single
# quotes are correct and SC2016's suggestion would break them.
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
  pin JIRA_IMAGE       atlassian/jira-software:9.17
  pin CONFLUENCE_IMAGE atlassian/confluence:latest
} > .env

echo "pin-images: wrote $(pwd)/.env"
