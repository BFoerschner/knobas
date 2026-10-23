#!/usr/bin/env bash
# The comparison half of `just ipc-mirror` (#1694):
#
#     ipc-mirror-check.sh <generated dir> <generator command...>
#
# Runs the generator and fails when it changed the working tree under the
# directory. The reference is the tree as it was *before* the run, not HEAD:
# `just pre-gate` regenerates the mirror and leaves it uncommitted for the
# agent to commit, and a check against HEAD reported that file stale on the
# gate that followed. A committed stale mirror still fails, because the run
# rewrites it; so does a file the run adds or removes.
set -euo pipefail

dir=$1
shift

snapshot() {
    # Path and content hash of every file under the directory.
    find "$dir" -type f -print0 2>/dev/null | LC_ALL=C sort -z | xargs -0 shasum 2>/dev/null || true
}

before=$(mktemp "${TMPDIR:-/tmp}/knobas-ipc-mirror.XXXXXX")
after=$(mktemp "${TMPDIR:-/tmp}/knobas-ipc-mirror.XXXXXX")
trap 'rm -f "$before" "$after"' EXIT
snapshot > "$before"

"$@"

snapshot > "$after"
if ! diff -q "$before" "$after" >/dev/null; then
    echo "error: the generated IPC mirror is stale; the Rust types now produce:" >&2
    diff "$before" "$after" | sed -n 's/^[<>] [0-9a-f]*  /  /p' | LC_ALL=C sort -u >&2
    echo "  Commit the regenerated files (the run above wrote them)." >&2
    exit 1
fi
echo "ipc-mirror: $dir matches the Rust types"
