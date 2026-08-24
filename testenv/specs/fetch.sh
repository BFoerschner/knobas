#!/bin/sh
# Re-fetch the vendored OpenAPI documents from their official sources.
# Run from this directory. After fetching, `git diff` shows what the vendors
# changed; update SHA256SUMS (`shasum -a 256 *.json > SHA256SUMS`) only after
# reviewing the diff — mockd's contract tests pin against these files.
set -eu
curl -fsSL -o jira-cloud-v3.json       "https://developer.atlassian.com/cloud/jira/platform/swagger-v3.v3.json"
curl -fsSL -o confluence-cloud-v1.json "https://developer.atlassian.com/cloud/confluence/swagger.v3.json"
curl -fsSL -o confluence-cloud-v2.json "https://developer.atlassian.com/cloud/confluence/openapi-v2.v3.json"
# teamcity.json is NOT fetched here: TeamCity publishes its spec only from a
# running server. Extract it from the pinned container instead:
#   docker run --rm -d --name tc-spec -p 127.0.0.1:8111:8111 jetbrains/teamcity-server:<pinned>
#   curl -fsSL http://127.0.0.1:8111/app/rest/swagger.json -o teamcity.json   (after startup)
#   docker rm -f tc-spec
shasum -a 256 -c SHA256SUMS || echo "==> specs changed upstream; review the diff before updating SHA256SUMS"
