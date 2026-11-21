#!/usr/bin/env bash
# The press trips' time limit (`trip-runner.mjs`, the trip-limit pass, 2026-11-20), run by `just witness-unit`: with
# stand-in browsers, a stuck trip fails by name at its limit, its browser is replaced, the run goes on and ends, and the
# progress line names each worker's trip. Under a second; no Chrome.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
exec node --test --test-timeout=30000 "$here"/test/*.test.mjs
