#!/bin/sh
# Seed the Forgejo container (#1195) with the Tidewater content, exactly as
# seed-gitea.sh seeds Gitea: this is that script pointed at the `forgejo`
# service on its own port, with its own state file. Idempotent, like it.
#
#   docker compose --env-file images.env --profile forgejo up -d --wait forgejo
#   ./seed-forgejo.sh
#   eval "$(./seed --env-forgejo)"     # KNOBAS_GITEA_* for the Gitea live suite
#
# `just gitea-live forgejo` does all three and runs the suite.
set -eu
cd "$(dirname "$0")"
export KNOBAS_GITEA_URL="${KNOBAS_FORGEJO_URL:-http://127.0.0.1:3002}"
export KNOBAS_SEED_SERVICE=forgejo
export KNOBAS_SEED_CLI=forgejo
export KNOBAS_SEED_STATE=seed-state-forgejo.json
exec ./seed-gitea.sh
