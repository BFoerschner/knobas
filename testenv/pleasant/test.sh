#!/usr/bin/env bash
# The Pleasant stand-in against the real server's recordings and at volume (testenv/pleasant/README.md), no VM needed.
# Part of `just witness-unit`.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
node --test "$here/test/standin.test.mjs"
