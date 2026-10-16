#!/usr/bin/env bash
# The cargo package selection for the crates this branch changes (#972).
#
# Prints `-p <crate> -p <crate> ...` for every `crates/<crate>/` with a file
# changed since the merge base with BASE (committed, uncommitted or untracked),
# `--workspace` when a workspace-wide input changed (root Cargo.toml/Cargo.lock,
# the toolchain pin, `.cargo/`), and nothing when no Rust input changed.
# A crate's directory name is its package name.
#
# Usage: scripts/changed-crates.sh [BASE]   (default origin/main)
set -euo pipefail
base=${1:-origin/main}
root=$(git rev-parse --show-toplevel)
mb=$(git -C "$root" merge-base "$base" HEAD)
changed=$({ git -C "$root" diff --name-only "$mb"; git -C "$root" ls-files --others --exclude-standard; } | sort -u)
crates=
while IFS= read -r f; do
  case $f in
    Cargo.toml|Cargo.lock|rust-toolchain*|.cargo/*) echo "--workspace"; exit 0 ;;
    crates/*/*)
      c=${f#crates/}; c=${c%%/*}
      case " $crates " in *" $c "*) ;; *) crates="$crates $c" ;; esac ;;
  esac
done <<< "$changed"
out=
for c in $crates; do [ -f "$root/crates/$c/Cargo.toml" ] && out="$out -p $c"; done
echo "${out# }"
