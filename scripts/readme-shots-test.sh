#!/usr/bin/env bash
# The README screenshots' own test, run by `just witness-unit`. Nothing here
# starts Chrome: the pictures' pinned clock and scale are `shot-test.sh`'s cases.
#
# scripts/readme-shots.sh, through its dry run:
# - the real shots.txt and README.md agree;
# - in a scratch tree that agrees (the control), each way they can stop agreeing
#   is refused, naming the picture, before any shooting.
#
# scripts/readme-shots-publish.sh, against a local bare repository and with no
# git identity configured, as on a release runner:
# - a release's pictures and stamp land on the branch, and the checkout stays put;
# - the same pictures again commit nothing;
# - an empty shots directory fails rather than reading as "nothing changed";
# - a push another merge beats is rebased and lands on top of that merge.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/readme-shots-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
failures=0
fail() { echo "FAIL: $1" >&2; failures=$((failures + 1)); }
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t

lines=$(grep -cE '^[a-z0-9-]+ ' "$here/../docs/screenshots/shots.txt")
if planned=$(README_SHOTS_DRY_RUN=1 "$here/readme-shots.sh" 9 2>&1); then
    [ "$(printf '%s\n' "$planned" | wc -l)" -eq "$lines" ] || fail "the real tree plans $planned, not one image for each of $lines lines"
else
    fail "the real shots.txt and README.md disagree: $planned"
fi

tree=$scratch/tree
mkdir -p "$tree/scripts" "$tree/docs/screenshots"
cp "$here/readme-shots.sh" "$here/shot-server.sh" "$tree/scripts/"
git -C "$tree" init -q -b main
shown='docs/screenshots/job.webp'
reset_tree() {
    printf '# a comment\n\njob  #/job/x + wait:1 + see:Some words\n' >"$tree/docs/screenshots/shots.txt"
    printf 'Pictures: %s\n' "$shown" >"$tree/README.md"
}
dry() { README_SHOTS_DRY_RUN=1 "$tree/scripts/readme-shots.sh" 9 2>&1; }
refused() { # refused <label> <what the message must say>
    local said
    if said=$(dry); then
        fail "$1 passed"
    else
        case $said in *"$2"*) ;; *) fail "$1 said: $said" ;; esac
    fi
    reset_tree
}
reset_tree
said=$(dry) || fail "a tree that agrees was refused: $said"
case $said in *"job.webp <- #/job/x + wait:1 + see:Some words"*) ;; *) fail "the route is not read whole: $said" ;; esac

printf 'Pictures: %s docs/screenshots/gone.webp\n' "$shown" >"$tree/README.md"
refused "a picture no line makes" "no line of shots.txt makes: gone.webp"
printf 'No pictures here.\n' >"$tree/README.md"
refused "a line the README does not show" "the README does not show: job.webp"
printf 'job  #/a + see:x\njob  #/b + see:x\n' >"$tree/docs/screenshots/shots.txt"
refused "a name twice" "'job' is there twice"
printf 'job\n' >"$tree/docs/screenshots/shots.txt"
refused "a line with no route" "'job' has no route"
printf 'job  #/job/x + wait:1\n' >"$tree/docs/screenshots/shots.txt"
refused "a line with nothing to see, which would photograph a page that is gone" "'job' names no text to see"
printf 'Job_1  #/a + see:x\n' >"$tree/docs/screenshots/shots.txt"
refused "a name a file should not carry" "'Job_1' is not a name"
printf '# only a comment\n' >"$tree/docs/screenshots/shots.txt"
refused "no shots" "shots.txt has no shots"

# A bare repository and two clones: the release's, holding the script, and the
# other merges'.
origin=$scratch/origin.git
git init -q --bare -b main "$origin"
clone=$scratch/clone
git clone -q "$origin" "$clone" 2>/dev/null
g() { git -C "$clone" -c user.name=t -c user.email=t@t "$@"; }
mkdir -p "$clone/scripts" "$clone/docs/screenshots"
cp "$here/readme-shots-publish.sh" "$clone/scripts/"
echo old >"$clone/docs/screenshots/job.webp"
echo 'As of <!-- shots-version -->v1.0.0<!-- /shots-version -->.' >"$clone/README.md"
g add -A
g commit -q -m base
g branch -q -M main
g push -q origin main
head_at() { git -C "$origin" rev-parse main; }
other=$scratch/other
git clone -q "$origin" "$other" 2>/dev/null

shots=$scratch/shots
mkdir "$shots"
echo new >"$shots/job.webp"
g checkout -q --detach
stood=$(g rev-parse HEAD)
"$clone/scripts/readme-shots-publish.sh" v1.1.0 "$shots" >/dev/null 2>"$scratch/err" || fail "a release's pictures did not publish: $(cat "$scratch/err")"
published=$(head_at)
[ "$published" != "$stood" ] || fail "a release's pictures made no commit"
[ "$(git -C "$origin" show main:docs/screenshots/job.webp)" = new ] || fail "the picture is not on the branch"
git -C "$origin" show main:README.md | grep -q 'shots-version -->v1.1.0<!--' || fail "the stamp does not name the release"
[ "$(git -C "$origin" log -1 --format=%s main)" = "Refresh the README's screenshots for v1.1.0" ] || fail "the commit's subject: $(git -C "$origin" log -1 --format=%s main)"
[ "$(git -C "$origin" diff --name-only "$stood" main | sort | tr '\n' ' ')" = "README.md docs/screenshots/job.webp " ] || fail "the commit holds more than the pictures and the stamp"
[ "$(g rev-parse HEAD)" = "$stood" ] || fail "the checkout moved off what it stood on"
[ "$(g worktree list | wc -l)" -eq 1 ] || fail "the worktree was left: $(g worktree list)"

"$clone/scripts/readme-shots-publish.sh" v1.1.0 "$shots" >"$scratch/out" 2>&1 || fail "the same pictures again failed: $(cat "$scratch/out")"
[ "$(head_at)" = "$published" ] || fail "the same pictures again made a commit"
grep -q "nothing to commit" "$scratch/out" || fail "the same pictures again said: $(cat "$scratch/out")"

mkdir "$scratch/empty"
if "$clone/scripts/readme-shots-publish.sh" v1.2.0 "$scratch/empty" >/dev/null 2>&1; then fail "an empty shots directory passed"; fi
[ "$(head_at)" = "$published" ] || fail "an empty shots directory made a commit"

# The merge that beats the push: a pre-push hook lands it on the branch the moment
# this push has read the branch, once.
mkdir "$scratch/hooks"
cat >"$scratch/hooks/pre-push" <<HOOK
#!/bin/sh
[ -e "$scratch/raced" ] && exit 0
touch "$scratch/raced"
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
git -C "$other" pull -q origin main
git -C "$other" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "another merge"
git -C "$other" push -q origin HEAD:main
HOOK
chmod +x "$scratch/hooks/pre-push"
g config core.hooksPath "$scratch/hooks"
echo newer >"$shots/job.webp"
"$clone/scripts/readme-shots-publish.sh" v1.2.0 "$shots" >/dev/null 2>"$scratch/err" || fail "a push another merge beat did not land: $(cat "$scratch/err")"
[ -e "$scratch/raced" ] || fail "the race never ran, so this case proved nothing"
[ "$(git -C "$origin" log -2 --format=%s main | tr '\n' '|')" = "Refresh the README's screenshots for v1.2.0|another merge|" ] \
    || fail "the commit is not on top of the merge that beat it: $(git -C "$origin" log -3 --format=%s main)"
[ "$(git -C "$origin" show main:docs/screenshots/job.webp)" = newer ] || fail "the rebased commit lost its picture"

if [ "$failures" -gt 0 ]; then echo "readme-shots-test: $failures failure(s)" >&2; exit 1; fi
echo "readme-shots-test: ok"
