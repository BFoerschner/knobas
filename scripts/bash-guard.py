#!/usr/bin/env python3
"""The machine rules of CLAUDE.md, enforced before a Bash command runs.

A `PreToolUse` hook (`.claude/settings.json`) hands this script the tool call
as JSON on stdin. A command of a shape that stops an unattended run for a
permission prompt is refused here instead, with exit status 2 and a message
naming the allowed form, so the agent rewrites it and goes on with nobody at
the machine. Everything else passes with status 0, and so does anything this
script cannot read: the hook fails open, never closed.

The shapes, each one a prompt or a rule of CLAUDE.md's *Machine* section:

* `cd` chained into a compound command (`cd x && y`, `(cd x; y)`, `$(cd x && pwd)`).
* `rm` with a glob (`rm -f $T/*.png` stopped a run on 2026-10-20). A path built
  from a variable alone is fine: hundreds of `rm -rf $W/target` ran unattended.
* A write, move or delete under `~/.claude/` or any `.git/` (git commands are fine).
* A write to a shell configuration file (`~/.bashrc`, `~/.zshrc`, ...).

Only the command line is read. Quoted text and heredoc bodies are data, never
commands, so a commit message or a script written through a heredoc that
mentions `cd` passes.

Test: scripts/bash-guard-test.sh, run by `just witness-unit`.
"""

import json
import re
import sys

WRITERS = {"rm", "rmdir", "mv", "cp", "touch", "mkdir", "ln", "tee", "truncate", "install", "chmod", "chown", "rsync"}
IN_PLACE = {"sed", "perl"}  # writers only with -i
WRAPPERS = {"sudo", "command", "exec", "time", "nohup", "nice", "xargs"}
SHELL_CONFIG = (".bashrc", ".bash_profile", ".profile", ".zshrc", ".zprofile", ".zshenv", ".zlogin")


class Word:
    """One shell word: its text, and whether it carries an expansion or an unquoted glob."""

    def __init__(self):
        self.text = ""
        self.expands = False  # a `$` or backquote outside single quotes
        self.globs = False  # `*`, `?` or `[` outside any quotes


def strip_heredocs(command):
    """The command line without the bodies of its heredocs."""
    lines = command.split("\n")
    out = []
    pending = []
    for line in lines:
        if pending:
            delimiter, dash = pending[0]
            if (line.lstrip("\t") if dash else line) == delimiter:
                pending.pop(0)
            continue
        out.append(line)
        for m in re.finditer(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2", line):
            pending.append((m.group(3), m.group(1) == "-"))
    return "\n".join(out)


def commands_of(command):
    """Split a command line into simple commands, each a list of Words.

    `;`, `&&`, `||`, `|`, `&`, newlines, parentheses and command substitution
    all start a new command. Redirection targets are kept as words beginning
    with `>`.
    """
    commands = [[]]
    word = None
    i = 0
    s = command

    def end_word():
        nonlocal word
        if word is not None:
            commands[-1].append(word)
            word = None

    def new_command():
        end_word()
        if commands[-1]:
            commands.append([])

    while i < len(s):
        c = s[i]
        if c == "\\" and i + 1 < len(s):
            word = word or Word()
            word.text += s[i + 1]
            i += 2
            continue
        if c == "'":
            j = s.find("'", i + 1)
            j = len(s) if j < 0 else j
            word = word or Word()
            word.text += s[i + 1:j]
            i = j + 1
            continue
        if c == '"':
            j = i + 1
            word = word or Word()
            while j < len(s) and s[j] != '"':
                if s[j] == "\\" and j + 1 < len(s):
                    word.text += s[j + 1]
                    j += 2
                    continue
                if s[j] in "$`":
                    word.expands = True
                word.text += s[j]
                j += 1
            i = j + 1
            continue
        if s.startswith("$(", i) and not s.startswith("$((", i):
            new_command()
            i += 2
            continue
        if c == "`":
            new_command()
            i += 1
            continue
        if c in ";|&\n()":
            new_command()
            i += 1
            continue
        if c in " \t":
            end_word()
            i += 1
            continue
        if c == ">":
            end_word()
            j = i
            while j < len(s) and s[j] in ">&|":
                j += 1
            word = Word()
            word.text = ">"
            i = j
            while i < len(s) and s[i] in " \t":
                i += 1
            continue
        word = word or Word()
        if c in "$":
            word.expands = True
        if c in "*?[":
            word.globs = True
        word.text += c
        i += 1
    end_word()
    return [cmd for cmd in commands if cmd]


def name_and_args(words):
    """The command's name and its arguments, past assignments and wrappers like `env -C dir`."""
    i = 0
    while i < len(words):
        text = words[i].text
        if re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", text):
            i += 1
            continue
        if text == "env":
            i += 1
            while i < len(words) and (words[i].text.startswith("-") or "=" in words[i].text):
                if words[i].text in ("-C", "-u", "-S"):
                    i += 1
                i += 1
            continue
        if text in WRAPPERS:
            i += 1
            continue
        break
    if i >= len(words):
        return None, []
    return words[i].text.rsplit("/", 1)[-1], words[i + 1:]


def protected(path):
    """Why a path is out of bounds for a Bash write, or None."""
    p = path.strip("'\"")
    # A background job's own scratch directory is ~/.claude/jobs/<id>/tmp.
    if re.search(r"/\.claude/jobs/[^/]+/tmp(/|$)", p):
        return None
    if re.search(r"(^|/)\.git(/|$)", p):
        return "a .git directory"
    if re.match(r"^(~|\$HOME|\$\{HOME\}|/Users/[^/]+|/home/[^/]+)/\.claude(/|$)", p):
        return "~/.claude"
    if re.match(r"^(~|\$HOME|\$\{HOME\}|/Users/[^/]+|/home/[^/]+)/(" + "|".join(re.escape(x) for x in SHELL_CONFIG) + r")$", p):
        return "a shell configuration file"
    return None


def verdict(command):
    """None when the command may run, else the message that refuses it."""
    cmds = commands_of(strip_heredocs(command))
    for words in cmds:
        name, args = name_and_args(words)
        if name in ("cd", "pushd") and len(cmds) > 1:
            return ("`cd` chained into a compound command stops the run for a permission prompt. "
                    "Use absolute paths, `git -C <dir>`, `npm --prefix <dir>`, or `env -C <dir> <cmd>` "
                    "for anything that must run in a directory (cargo, just).")
        if name == "rm":
            for a in args:
                if not a.text.startswith("-") and a.globs:
                    return ("`rm` with a glob stops the run for a permission prompt "
                            f"(`{a.text}`). Name each file, or leave them.")
        writes = name in WRITERS or (name in IN_PLACE and any(a.text.startswith("-i") or a.text == "--in-place" for a in args))
        targets = [a.text for a in args if not a.text.startswith("-")] if writes else []
        targets += [a.text[1:] for a in words if a.text.startswith(">") and len(a.text) > 1]
        for t in targets:
            why = protected(t)
            if why:
                return (f"A Bash write under {why} (`{t}`) stops the run for a permission prompt. "
                        "Git commands are fine; memory files change through Write/Edit; skip anything else and say so.")
    return None


def main():
    try:
        call = json.load(sys.stdin)
        command = call.get("tool_input", {}).get("command")
    except (ValueError, AttributeError):
        return 0
    if call.get("tool_name") != "Bash" or not isinstance(command, str):
        return 0
    message = verdict(command)
    if message is None:
        return 0
    print(f"bash-guard: {message} (CLAUDE.md, Machine)", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
