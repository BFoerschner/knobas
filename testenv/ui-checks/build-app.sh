#!/usr/bin/env bash
# Builds a checkout's app into a folder, in development mode so ?fake-ipc still works.
# Usage: build-app.sh <checkout> <folder>     <folder> is an absolute path outside the checkout; it is emptied first.
set -uo pipefail
checkout=$1 folder=$2
case $folder in
  /*/*) ;;
  *) echo "build-app: <folder> must be an absolute path below a directory, not '$folder'" >&2; exit 2 ;;
esac
case $folder/ in
  "${checkout%/}"/*) echo "build-app: <folder> must be outside the checkout" >&2; exit 2 ;;
esac
env -C "$checkout/app" NODE_ENV=development npx vite build --outDir "$folder" --emptyOutDir --logLevel warn || exit $?
# A production build folds the fake bridge away and every screen would come up empty, so say so now.
ls "$folder"/assets/fake-tauri-*.js >/dev/null 2>&1 || { echo "build-app: no fake-tauri chunk in $folder/assets, so ?fake-ipc would not work" >&2; exit 1; }
echo "built $(git -C "$checkout" rev-parse --short HEAD) into $folder"
