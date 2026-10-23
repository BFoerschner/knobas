#!/usr/bin/env python3
"""Hold the hand-written tables that list a part of the tree to that tree (#1919).

Each table below names, in its first column, every member of something the
tree holds. Nothing else connects them: `members = ["crates/*"]` is a glob, a
lib directory or a `*-live` recipe is added without touching the doc, and three
crates went without a row for months. Each check runs both ways: a member with
no row, and a row naming no member.

  docs/development.md   Crates     every directory under crates/ with a Cargo.toml
  docs/development.md   Frontend   every directory under app/src/lib/; every
                                   `app/...` path a row names exists
  testenv/README.md     live suites  every justfile recipe named `<x>-live[-<y>]`
  testenv/README.md     live-up    every product `_live_main` in testenv/live-lib.sh knows

The tree is what git lists (tracked, or untracked and not ignored) and is still
on disk, so an ignored `target/` or a directory a branch switch left empty does
not count. Exit 1 naming each miss. Run with `python3 -I scripts/doc-lists.py`
(in `just docs-check`, `check` and `check-front`).
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEV = "docs/development.md"
TESTENV = "testenv/README.md"


def tree_files():
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        check=True, capture_output=True, text=True,
    ).stdout
    return [f for f in out.split("\0") if f and (ROOT / f).is_file()]


def first_column(doc, header):
    """The backticked names in the first cell of each row of the table whose
    header row starts with `header`. Raises if there is not exactly one."""
    lines = (ROOT / doc).read_text().splitlines()
    starts = [i for i, l in enumerate(lines) if l.startswith(header)]
    if len(starts) != 1:
        raise LookupError(f"{doc}: {len(starts)} tables start `{header}`, wanted 1")
    rows = []
    for line in lines[starts[0] + 1 :]:
        if not line.startswith("|"):
            break
        cell = line.split("|")[1]
        if re.fullmatch(r"\s*:?-+:?\s*", cell):
            continue
        rows.append(re.findall(r"`([^`]+)`", cell))
    return rows


def expand(path):
    """`a/{b,c}/` -> [`a/b/`, `a/c/`]; one brace group is all the tables use."""
    m = re.search(r"\{([^{}]*)\}", path)
    if not m:
        return [path]
    return [path[: m.start()] + part + path[m.end() :] for part in m.group(1).split(",")]


def both_ways(where, have, named, what):
    """Misses between the members the tree has and the names the table gives."""
    misses = [f"{where}: `{n}` has no row ({what})" for n in sorted(have - named)]
    misses += [f"{where}: the row `{n}` names nothing that exists ({what})" for n in sorted(named - have)]
    return misses


def crates(files):
    have = {m.group(1) for f in files if (m := re.fullmatch(r"crates/([^/]+)/Cargo\.toml", f))}
    named = {n for row in first_column(DEV, "| Crate | What it is |") for n in row}
    return both_ways(f"{DEV}, Crates", have, named, "a directory under crates/ with a Cargo.toml")


def frontend(files):
    misses = []
    for row in first_column(DEV, "| Path | What lives there |"):
        for path in (p for n in row for p in expand(n)):
            if path.endswith("/"):
                ok = any(f.startswith(path) for f in files)
            else:
                ok = path in files
            if not ok:
                misses.append(f"{DEV}, Frontend: the row `{path}` names nothing that exists")
    dirs = {m.group(1) for f in files if (m := re.match(r"(app/src/lib/[^/]+/)", f))}
    named = {p for row in first_column(DEV, "| Path | What lives there |") for n in row for p in expand(n)}
    misses += [f"{DEV}, Frontend: `{d}` has no row (a directory under app/src/lib/)" for d in sorted(dirs - named)]
    return misses


def live_suites(_files):
    recipes = set()
    for line in (ROOT / "justfile").read_text().splitlines():
        m = re.match(r"([a-z][a-z0-9-]*)(\s[^:]*)?:(?!=)", line)
        if m and re.search(r".-live(-|$)", m.group(1)):
            recipes.add(m.group(1))
    named = set()
    for row in first_column(TESTENV, "| Recipe | Suite |"):
        named |= {n.removeprefix("just ") for n in row if n.startswith("just ")}
    return both_ways(f"{TESTENV}, live suites", recipes, named, "a justfile recipe named <x>-live")


def live_products(_files):
    text = (ROOT / "testenv/live-lib.sh").read_text()
    body = re.search(r"^_live_main\(\) \{\n(.*?)^\}", text, re.M | re.S)
    if not body:
        raise LookupError("testenv/live-lib.sh: no `_live_main() {` to read the products from")
    have = set(re.findall(r"^\s+([a-z0-9-]+)\) echo ", body.group(1), re.M))
    if not have:
        raise LookupError("testenv/live-lib.sh: `_live_main` names no product")
    named = {n for row in first_column(TESTENV, "| Product | `live-up` starts |") for n in row}
    return both_ways(f"{TESTENV}, live-up", have, named, "a product testenv/live-lib.sh's _live_main knows")


def main():
    files = tree_files()
    misses = []
    for check in (crates, frontend, live_suites, live_products):
        try:
            misses += check(files)
        except LookupError as e:
            misses.append(str(e))
    if misses:
        print("doc-lists: FAILED, a hand-written table and the tree disagree:", file=sys.stderr)
        for m in misses:
            print(f"  {m}", file=sys.stderr)
        print("doc-lists: add or remove the row (or fix the table's header the check finds it by)", file=sys.stderr)
        return 1
    print("doc-lists: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
