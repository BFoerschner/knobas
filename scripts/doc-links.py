#!/usr/bin/env python3
"""Hold every Markdown link in the tree to a file and an anchor that exist.

A link nothing checks goes dead and stays dead. The glossary's `[job](#job)`
pointed at anchors that never existed, because a term is bold text and
Markdown makes anchors for headings only: 184 links in `CONTEXT.md` and 24
more from the ADRs. An archived file kept 307 links to a folder that had moved.
Neither failed anything.

In every Markdown file git lists (tracked, or untracked and not ignored):

  [text](target)   ![alt](target)   [ref]: target   src=""   href=""   srcset=""

  - a relative target names a file or a directory of the tree
  - `#anchor` on a Markdown target names a heading of that file, slugged as
    GitHub slugs it, or an `<a id="...">` or `<a name="...">` in it

Not checked: a target with a scheme (`https:`, `mailto:`), a link inside a
code span or a code fence, an anchor on a file that is not Markdown
(`main.rs#L10`), and any file under a `fixtures/` directory, which is data.

The tree is what git lists and is still on disk, as in `scripts/doc-lists.py`:
a link to an ignored file works on this machine and nowhere else, so it fails.

Exit 1 naming each miss as `file:line`. Run with `python3 -I scripts/doc-links.py`
(in `just docs-check`, `check` and `check-front`).
"""
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parent.parent

FENCE = re.compile(r"^\s{0,3}(`{3,}|~{3,})")
CODE_SPAN = re.compile(r"(`+)(?!`).*?(?<!`)\1(?!`)")
HEADING = re.compile(r"^\s{0,3}#{1,6}\s+(.*?)(?:\s+#+)?\s*$")
INLINE_LINK = re.compile(r"\]\(\s*(?:<([^>]*)>|([^)\s]+))(?:\s+(?:\"[^\"]*\"|'[^']*'))?\s*\)")
REFERENCE = re.compile(r"^\s{0,3}\[(?!\^)[^\]]+\]:\s*(?:<([^>]*)>|(\S+))")
ATTRIBUTE = re.compile(r"""\b(?:src|href)\s*=\s*["']([^"']+)["']""")
SRCSET = re.compile(r"""\bsrcset\s*=\s*["']([^"']+)["']""")
ANCHOR_TAG = re.compile(r"""<a\s[^>]*\b(?:id|name)\s*=\s*["']([^"']+)["']""")
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


def tree_files():
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        check=True, capture_output=True, text=True,
    ).stdout
    return [f for f in out.split("\0") if f and (ROOT / f).is_file()]


def prose_lines(text):
    """(line number, line) for each line outside a code fence."""
    fence = None
    for number, line in enumerate(text.splitlines(), 1):
        m = FENCE.match(line)
        if m:
            mark = m.group(1)
            if fence is None:
                fence = mark[0] * len(mark)
            elif mark[0] == fence[0] and len(mark) >= len(fence):
                fence = None
            continue
        if fence is None:
            yield number, line


def slug(heading):
    """The anchor GitHub gives a heading: its text in lower case, with every
    character dropped that is not a letter, a digit, `_`, `-` or a space, and
    each space made a hyphen. `A — b` is `a--b`: the dash goes, both spaces stay."""
    text = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", heading)  # a link or an image is its text
    text = re.sub(r"<[^>]+>", "", text)                        # an HTML tag is nothing
    text = text.replace("`", "")
    text = re.sub(r"\*+", "", text)                            # emphasis marks
    text = re.sub(r"(?<!\w)_+(?=\S)(.+?)(?<=\S)_+(?!\w)", r"\1", text)
    text = re.sub(r"[^\w\- ]", "", text.strip().lower())
    return text.replace(" ", "-")


def anchors_of(path, cache={}):
    """Every anchor a Markdown file answers to: its headings' slugs, a repeat
    taking `-1`, `-2` as GitHub numbers them, and its `<a id>` and `<a name>`."""
    if path not in cache:
        found, seen = set(), {}
        for _, line in prose_lines((ROOT / path).read_text(errors="replace")):
            m = HEADING.match(line)
            if m:
                s = slug(m.group(1))
                n = seen.get(s, 0)
                seen[s] = n + 1
                found.add(s if n == 0 else f"{s}-{n}")
            found.update(ANCHOR_TAG.findall(line))
        cache[path] = found
    return cache[path]


def targets_in(line):
    """Each link target a line of prose names, code spans left out."""
    bare = CODE_SPAN.sub("", line)
    for a, b in INLINE_LINK.findall(bare):
        yield a or b
    m = REFERENCE.match(bare)
    if m:
        yield m.group(1) or m.group(2)
    yield from ATTRIBUTE.findall(line)
    for candidates in SRCSET.findall(line):
        for candidate in candidates.split(","):
            if candidate.split():
                yield candidate.split()[0]


def resolve(source, target):
    """The tree path a relative target names from `source`, or None above the root."""
    parts = [] if target.startswith("/") else list(Path(source).parent.parts)
    for part in target.split("/"):
        if part in ("", "."):
            continue
        if part == "..":
            if not parts:
                return None
            parts.pop()
        else:
            parts.append(part)
    return "/".join(parts)


def main():
    files = tree_files()
    listed = set(files)
    directories = {str(parent) for f in files for parent in Path(f).parents if str(parent) != "."}
    documents = [f for f in files if f.endswith(".md") and "fixtures" not in Path(f).parts]

    misses, links = [], 0
    for doc in documents:
        for number, line in prose_lines((ROOT / doc).read_text(errors="replace")):
            for target in targets_in(line):
                target = target.strip()
                if not target or SCHEME.match(target) or target.startswith("//"):
                    continue
                links += 1
                path, _, fragment = target.partition("#")
                path = unquote(path.split("?", 1)[0])
                if path:
                    file = resolve(doc, path)
                    if file is None or (file not in listed and file not in directories and file != ""):
                        misses.append(f"{doc}:{number}: {target} (no such file)")
                        continue
                else:
                    file = doc
                if fragment and file.endswith(".md") and file in listed:
                    if unquote(fragment) not in anchors_of(file):
                        misses.append(f"{doc}:{number}: {target} (no heading or `<a id>` of {file} makes that anchor)")

    if misses:
        print("doc-links: FAILED, a link names nothing:", file=sys.stderr)
        for m in misses:
            print(f"  {m}", file=sys.stderr)
        print(
            "doc-links: fix the link, or give the target a heading or an `<a id>` (a glossary term in "
            "CONTEXT.md carries `<a id>`, because bold text makes no anchor)",
            file=sys.stderr,
        )
        return 1
    print(f"doc-links: ok ({links} links in {len(documents)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
