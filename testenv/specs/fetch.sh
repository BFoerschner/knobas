#!/bin/sh
# Re-fetch the vendored API contract documents from their official sources.
# Run from this directory. After fetching, `git diff` shows what changed;
# update SHA256SUMS (`shasum -a 256 *.json *.wadl > SHA256SUMS`) only after
# reviewing the diff — mockd's contract tests pin against these files.
set -eu

# ---- TeamCity ----
#
# #############################################################################
# ## STOP. RUNNING THIS OVERWRITES A HAND-AUGMENTED FILE AND LOSES WORK.      ##
# #############################################################################
#
# `teamcity.json` is NOT a pure download. It was taken from JetBrains' guest
# server instance (TeamCity 2026.1) and then **hand-augmented by Björn** with
# information from the official HTML documentation. Those additions exist
# nowhere else -- not upstream, not on any server, not in any other file. The
# `curl -o teamcity.json` below would silently destroy them.
#
# So: NEVER run this straight onto the vendored copy. Fetch to a scratch path
# and diff:
#
#     ./fetch.sh --teamcity --to /tmp/teamcity-fresh.json
#     diff <(jq -S . teamcity.json) <(jq -S . /tmp/teamcity-fresh.json)
#
# and then merge upstream's changes INTO the vendored file by hand, keeping the
# augmentations. `git diff` is not a safety net here: if you overwrite and
# commit, the additions are gone from the working tree, and recovering them
# means knowing they ever existed. That is what this comment is for.
# See testenv/specs/README.md, "mixed authority".
#
# ---- Where the document actually comes from ----
# JetBrains publishes no static swagger document at a public documentation URL
# (re-verified 2026-08-25: four plausible URLs answer 404/403). But the earlier
# conclusion drawn from that -- "the only source is a server you run yourself"
# -- was too pessimistic, and cost two days of blocked gate. The document is
# served by ANY running TeamCity, including **JetBrains' own public guest
# instance**, which needs no Docker pull, no 10 GB of disk and no first-start
# wizard. That is how the vendored copy was obtained (2026-08-27).
#
# The self-hosted route below is kept as the fallback for when a specific
# version is needed that the guest instance does not run. It is behind a
# one-time first-start wizard -- database choice, licence agreement,
# administrator account -- served by form endpoints that are not REST API and
# that change between versions. It is a *human* action, so this is NOT part of
# the default run: `./fetch.sh` stays a pure curl of the public documents, with
# no Docker and no interaction.
#
# Run it deliberately, with a human at the browser:
#     ./fetch.sh --teamcity --to <scratch path>
fetch_teamcity() {
  # Default kept OUT of the vendored path on purpose: an accidental
  # `./fetch.sh --teamcity` writes a scratch file, not teamcity.json.
  out=${1:-./teamcity-fresh.json}

  # RESOLVE both sides before comparing. Matching the *spelling* would be the
  # very bug this guard exists to prevent, one level up: `teamcity.json` and
  # `./teamcity.json` are two of the ways to name this file, and
  # `testenv/specs/teamcity.json` from the repo root -- the unremarkable
  # invocation -- is another, as are `../specs/teamcity.json` and any absolute
  # path. Comparing the resolved directory plus basename catches every spelling
  # of the same file: `..` segments, symlinks (`pwd -P`) and absolute paths.
  vendored_dir=$(cd "$(dirname "$0")" && pwd -P) || return 1
  out_dir=$(cd "$(dirname "$out")" 2>/dev/null && pwd -P) || out_dir=

  # Unresolvable target directory => REFUSE, not proceed. A guard that cannot
  # tell where the write would land must not wave it through; `curl -o` would
  # fail on such a path anyway, so the only thing fail-open buys here is a worse
  # error message and a guard whose behaviour depends on what happens to exist.
  if [ -z "$out_dir" ]; then
    echo "==> refusing: cannot resolve the directory for '$out' (does it exist?)." >&2
    echo "==> name an existing scratch directory so this guard can check the target." >&2
    return 2
  fi
  if [ "$out_dir/$(basename "$out")" = "$vendored_dir/teamcity.json" ]; then
    echo "==> refusing to write $vendored_dir/teamcity.json: it is hand-augmented." >&2
    echo "==> you named it '$out'; that resolves to the vendored copy." >&2
    echo "==> fetch to a scratch path and diff. See the comment above." >&2
    return 2
  fi
  echo "==> fetching to $out (NOT the vendored teamcity.json -- diff before merging)"
  # Budget: a multi-GB image pull and a few minutes of JVM startup. Needs
  # roughly 10 GB of free disk for the image plus its data directory.
  docker pull jetbrains/teamcity-server:latest
  echo "==> pin this digest in ../.env and in the row below:"
  docker image inspect --format '{{index .RepoDigests 0}}' jetbrains/teamcity-server:latest
  docker run --rm -d --name tc-spec -p 127.0.0.1:8111:8111 jetbrains/teamcity-server:latest

  echo "==> waiting for the server to answer on 8111 ..."
  until curl -fsS -o /dev/null http://127.0.0.1:8111/; do sleep 5; done
  docker exec tc-spec cat /opt/teamcity/webapps/ROOT/WEB-INF/DistributionType.txt || true

  cat <<'WIZARD'
==> HUMAN STEP. Open http://127.0.0.1:8111/ and click through:
      Proceed (internal HSQLDB) -> accept the licence agreement
      -> create the administrator `mockd-spec` with a throwaway password.
    Then re-run with the password:  TC_PASSWORD=<password> ./fetch.sh --teamcity
    Do not script this: the wizard's form endpoints are not REST API and
    change between versions.
WIZARD
  [ -n "${TC_PASSWORD:-}" ] || { echo "==> TC_PASSWORD not set; stopping after the wizard step."; return 1; }

  TOKEN=$(curl -fsS -u "mockd-spec:$TC_PASSWORD" -X POST \
    -H 'Accept: application/json' \
    http://127.0.0.1:8111/app/rest/users/username:mockd-spec/tokens/spec | jq -r .value)
  curl -fsSL -H "Authorization: Bearer $TOKEN" -H 'Accept: application/json' \
    http://127.0.0.1:8111/app/rest/swagger.json -o "$out"
  docker rm -f tc-spec

  # Verify before pinning. A document that does not carry the schemas mockd is
  # validated against is worse than no document at all. The 2026.1 guest
  # instance emits OpenAPI 3.0.0 with LOWERCASE schema names under
  # components.schemas; older servers emitted Swagger 2.0 with capitalised
  # names under definitions. Accept either, and say which.
  jq -r '(.openapi // .swagger), .info.version, (.paths|keys|length)' "$out"
  jq -e '.paths["/app/rest/server"], .paths["/app/rest/builds"], .paths["/app/rest/buildTypes"]' "$out" >/dev/null
  if jq -e 'has("openapi")' "$out" >/dev/null; then
    echo "==> OpenAPI 3: $(jq -r '.components.schemas|keys|length' "$out") schemas under components.schemas"
    jq -e '.components.schemas.builds, .components.schemas.build, .components.schemas.buildTypes' "$out" >/dev/null
  else
    echo "==> Swagger 2.0: $(jq -r '.definitions|keys|length' "$out") definitions"
    jq -e '.definitions.Builds, .definitions.Build, .definitions.BuildTypes' "$out" >/dev/null
    echo "==> NOTE: the vendored copy is OpenAPI 3. A Swagger 2.0 re-fetch is a DOWNGRADE;"
    echo "    crates/knobas-mockd/tests/it/teamcity_contract.rs reads components.schemas."
  fi
  echo "==> ok, and NOT yet vendored. Next:"
  echo "    diff <(jq -S . teamcity.json) <(jq -S . $out)   # keep Björn's hand-added parts"
  echo "    then merge by hand, shasum -a 256 *.json *.wadl > SHA256SUMS, update README.md."
}

if [ "${1:-}" = "--teamcity" ]; then
  # `--to <path>` so the destination is always explicit at the call site.
  if [ "${2:-}" = "--to" ]; then
    fetch_teamcity "${3:?--to needs a path}"
  else
    fetch_teamcity
  fi
  exit $?
fi


# ---- Self-hosted (Data Center) — the PRIMARY target ----
# Jira DC WADL ("latest" Atlassian publishes; swap in a pinned version once
# Björn's instance version is known, e.g. .../REST/9.12.0/jira-rest-plugin.wadl):
curl -fsSL -o jira-dc-rest.wadl "https://docs.atlassian.com/software/jira/docs/api/REST/latest/jira-rest-plugin.wadl"
# Confluence DC: NO machine-readable spec is published anywhere (verified 2026-08-24).
# Contract = official HTML docs + the real atlassian/confluence container (--profile real-atlassian).

# ---- Cloud flavor (later) ----
curl -fsSL -o jira-cloud-v3.json       "https://developer.atlassian.com/cloud/jira/platform/swagger-v3.v3.json"
curl -fsSL -o confluence-cloud-v1.json "https://developer.atlassian.com/cloud/confluence/swagger.v3.json"
curl -fsSL -o confluence-cloud-v2.json "https://developer.atlassian.com/cloud/confluence/openapi-v2.v3.json"

shasum -a 256 -c SHA256SUMS || echo "==> contracts changed upstream; review the diff before updating SHA256SUMS"
