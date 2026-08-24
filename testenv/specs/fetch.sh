#!/bin/sh
# Re-fetch the vendored API contract documents from their official sources.
# Run from this directory. After fetching, `git diff` shows what changed;
# update SHA256SUMS (`shasum -a 256 *.json *.wadl > SHA256SUMS`) only after
# reviewing the diff — mockd's contract tests pin against these files.
set -eu

# ---- Self-hosted (Data Center) — the PRIMARY target ----
# Jira DC WADL ("latest" Atlassian publishes; swap in a pinned version once
# Björn's instance version is known, e.g. .../REST/9.12.0/jira-rest-plugin.wadl):
curl -fsSL -o jira-dc-rest.wadl "https://docs.atlassian.com/software/jira/docs/api/REST/latest/jira-rest-plugin.wadl"
# Confluence DC: NO machine-readable spec is published anywhere (verified 2026-08-24).
# Contract = official HTML docs + the real atlassian/confluence container (--profile real-atlassian).

# ---- TeamCity: spec only served by a running server ----
#   docker run --rm -d --name tc-spec -p 127.0.0.1:8111:8111 jetbrains/teamcity-server:<pinned>
#   curl -fsSL http://127.0.0.1:8111/app/rest/swagger.json -o teamcity.json   (after startup)
#   docker rm -f tc-spec

# ---- Cloud flavor (later) ----
curl -fsSL -o jira-cloud-v3.json       "https://developer.atlassian.com/cloud/jira/platform/swagger-v3.v3.json"
curl -fsSL -o confluence-cloud-v1.json "https://developer.atlassian.com/cloud/confluence/swagger.v3.json"
curl -fsSL -o confluence-cloud-v2.json "https://developer.atlassian.com/cloud/confluence/openapi-v2.v3.json"

shasum -a 256 -c SHA256SUMS || echo "==> contracts changed upstream; review the diff before updating SHA256SUMS"
