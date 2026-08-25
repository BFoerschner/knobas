#!/bin/sh
# Re-fetch the vendored API contract documents from their official sources.
# Run from this directory. After fetching, `git diff` shows what changed;
# update SHA256SUMS (`shasum -a 256 *.json *.wadl > SHA256SUMS`) only after
# reviewing the diff — mockd's contract tests pin against these files.
set -eu

# ---- TeamCity: spec only served by a running server ----
# JetBrains publishes no static swagger document anywhere (re-verified
# 2026-08-25: the four plausible public URLs answer 404/403). The only source is
# a running server's own `/app/rest/swagger.json`, which is behind
# authentication, which is behind a one-time first-start wizard. That wizard is
# a *human* action -- database choice, licence agreement, administrator account
# -- served by form endpoints that are not part of the REST API and that change
# between versions. So this is NOT part of the default run: `./fetch.sh` stays a
# pure curl of the public documents, with no Docker and no interaction.
#
# Run it deliberately, with a human at the browser:
#     ./fetch.sh --teamcity
fetch_teamcity() {
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
    http://127.0.0.1:8111/app/rest/swagger.json -o teamcity.json
  docker rm -f tc-spec

  # Verify before pinning. A document that does not carry the definitions
  # mockd is validated against is worse than no document at all.
  jq -r '.swagger, .info.version, (.paths|keys|length), (.definitions|keys|length)' teamcity.json
  jq -e '.paths["/app/rest/server"], .paths["/app/rest/builds"], .paths["/app/rest/buildTypes"]' teamcity.json >/dev/null
  jq -e '.definitions.Builds, .definitions.Build, .definitions.BuildTypes' teamcity.json >/dev/null
  echo "==> ok. Now: shasum -a 256 *.json *.wadl > SHA256SUMS, review the diff, update README.md."
  echo "==> if .swagger was absent and .openapi present, the server emits OpenAPI 3:"
  echo "    record that in README.md -- schemas live under components.schemas, not definitions,"
  echo "    and crates/knobas-mockd/tests/teamcity_contract.rs must follow."
}

if [ "${1:-}" = "--teamcity" ]; then
  fetch_teamcity
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
