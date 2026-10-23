#!/usr/bin/env python3
"""The machine rules of CLAUDE.md, enforced before a Bash command runs.

A `PreToolUse` hook (`.claude/settings.json`) hands this script the tool call
as JSON on stdin. A command of a shape that stops an unattended run for a
permission prompt is refused here instead, with exit status 2 and a message
naming the allowed form, so the agent rewrites it and goes on with nobody at
the machine. Everything else passes with status 0, and so does anything this
script cannot read: the hook fails open, never closed. A missing script fails
open too: the wiring checks for it and lets the call through with a note on
stderr (status 1), since Python's own "can't open file" is status 2.

The shapes, each one a prompt or a rule of CLAUDE.md's *Machine* section:

* `cd` chained into a compound command (`cd x && y`, `(cd x; y)`, `$(cd x && pwd)`,
  `{ cd x; y; }`, `if cd x; then y; fi`, `for d in a; do cd $d; done`, `builtin cd x && y`).
  A substitution in cd's own argument is no second command: `cd "$(git rev-parse --show-toplevel)"`.
* `rm` with a glob (`rm -f $T/*.png` stopped a run on 2026-10-20). A path built
  from a variable alone is fine: hundreds of `rm -rf $W/target` ran unattended.
* A write, move or delete under `~/.claude/` or any `.git/` (git commands are fine).
* A write to a shell configuration file (`~/.bashrc`, `~/.zshrc`, ...).

Only the command line is read, the way the shell reads it: quotes first. Quoted
text and heredoc bodies are data, never commands, so a commit message or a
script written through a heredoc that mentions `cd` passes; only an unquoted
`<<` that is not `<<<` opens a heredoc. What runs as a command is read as one:
a `$(...)` or backquote (quoted or not), and the string of an `eval` or `sh -c`.

Test: scripts/bash-guard-test.sh, run by `just witness-unit`.
"""

import json
import re
import sys

WRITERS = {"rm", "rmdir", "mv", "cp", "touch", "mkdir", "ln", "tee", "truncate", "install", "chmod", "chown", "rsync"}
# Writers only in place: `-i` alone or bundled after the flags that take no argument
# (`perl -pi`, `sed -Ei`), or sed's `--in-place`. `perl -MList::Util` is no `-i`.
IN_PLACE = {"sed": r"^(-[nErsuzal]*i|--in-place)", "perl": r"^-[aclnpstuwTUWX0-9]*i"}
# Words before the command's name, each with the options of its own that take an argument.
WRAPPERS = {"sudo": {"-u", "-g"}, "command": set(), "exec": set(), "time": set(), "nohup": set(),
            "nice": {"-n"}, "xargs": {"-I", "-n", "-P", "-L", "-d", "-E", "-s"}}
# Reserved words and `builtin` lead into the command they hold: `if cd x`, `do cd x`, `{ cd x`.
KEYWORDS = {"{", "}", "!", "if", "then", "else", "elif", "fi", "do", "done", "while", "until", "builtin"}
SHELLS = {"bash", "sh", "zsh", "dash", "ksh"}
MAX_DEPTH = 8  # eval and `sh -c` nested deeper than this are not read
SHELL_CONFIG = (".bashrc", ".bash_profile", ".profile", ".zshrc", ".zprofile", ".zshenv", ".zlogin")


class Word:
    """One shell word: its text, whether it carries an expansion or an unquoted glob,
    and the command lists of the substitutions inside it."""

    def __init__(self):
        self.text = ""
        self.expands = False  # a `$` or backquote outside single quotes
        self.globs = False  # `*`, `?` or `[` outside any quotes
        self.subs = []  # one list of commands per `$(...)` or backquote in the word


class Parser:
    """A shell command line read the way the shell reads it, quotes first.

    Quoting is known at every character, so only an unquoted `<<` that is not
    `<<<` opens a heredoc, and its body (the lines after the next unquoted
    newline, up to the delimiter) is skipped as data. A `$(...)` or backquote,
    quoted or not, is parsed as commands of its own.
    """

    def __init__(self, text):
        self.s = text
        self.pending = []  # heredocs opened on the current line: (delimiter, strip tabs)

    def parse(self, i, stop=None):
        """The commands from `i` up to `stop` (`)` or a backquote; None runs to the end), and the index past it."""
        s = self.s
        commands = [[]]
        word = None
        depth = 0  # plain parentheses opened inside this substitution

        def current():
            nonlocal word
            word = word or Word()
            return word

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
                if s[i + 1] != "\n":  # a backslash-newline joins the lines
                    current().text += s[i + 1]
                i += 2
                continue
            if (stop == "`" and c == "`") or (stop == ")" and c == ")" and depth == 0):
                end_word()
                return [cmd for cmd in commands if cmd], i + 1
            if c == "'":
                j = s.find("'", i + 1)
                j = len(s) if j < 0 else j
                current().text += s[i + 1:j]
                i = j + 1
                continue
            if s.startswith("$'", i):
                i = self.ansi_quoted(i + 2, current())
                continue
            if c == '"':
                i = self.double_quoted(i + 1, current())
                continue
            if s.startswith("$((", i):
                i = self.arithmetic(i, current())
                continue
            if s.startswith("$(", i) or c == "`":
                i = self.substitution(i, current())
                continue
            if c == "#" and word is None:  # a comment runs to the end of the line
                j = s.find("\n", i)
                i = len(s) if j < 0 else j
                continue
            if c == "\n":
                new_command()
                i = self.heredoc_bodies(i + 1)
                continue
            if c in ";|&()":
                if c == "(":
                    depth += 1
                elif c == ")" and depth:
                    depth -= 1
                new_command()
                i += 1
                continue
            if c in " \t":
                end_word()
                i += 1
                continue
            if s.startswith("<<", i):  # `<<<`, a here-string, reads an empty delimiter: no heredoc
                end_word()
                i = self.heredoc(i + 2)
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
            w = current()
            if c == "$":
                w.expands = True
            if c in "*?[":
                w.globs = True
            w.text += c
            i += 1
        end_word()
        return [cmd for cmd in commands if cmd], i

    def double_quoted(self, i, word):
        """Read a double-quoted string from `i` (past the quote) into `word`; the index past the closing quote."""
        s = self.s
        while i < len(s) and s[i] != '"':
            if s[i] == "\\" and i + 1 < len(s):
                if s[i + 1] in '$`"\\':
                    word.text += s[i + 1]
                elif s[i + 1] != "\n":
                    word.text += s[i:i + 2]
                i += 2
                continue
            if s.startswith("$((", i):
                i = self.arithmetic(i, word)
                continue
            if s.startswith("$(", i) or s[i] == "`":
                i = self.substitution(i, word)
                continue
            if s[i] == "$":
                word.expands = True
            word.text += s[i]
            i += 1
        return i + 1

    def ansi_quoted(self, i, word):
        """Read a `$'...'` string from `i` (past the quote), where a backslash escapes a quote."""
        s = self.s
        while i < len(s) and s[i] != "'":
            if s[i] == "\\" and i + 1 < len(s):
                word.text += s[i:i + 2]
                i += 2
                continue
            word.text += s[i]
            i += 1
        return i + 1

    def arithmetic(self, i, word):
        """Read a `$((...))` at `i` into `word` as one expansion: its `<<` is a shift, not a heredoc."""
        s = self.s
        j, depth = i + 1, 0
        while j < len(s):
            depth += {"(": 1, ")": -1}.get(s[j], 0)
            j += 1
            if depth == 0:
                break
        word.text += s[i:j]
        word.expands = True
        return j

    def substitution(self, i, word):
        """Parse the `$(...)` or backquote at `i` as commands of its own, kept on `word`."""
        if self.s[i] == "`":
            commands, j = self.parse(i + 1, "`")
        else:
            commands, j = self.parse(i + 2, ")")
        word.text += self.s[i:j]
        word.expands = True
        word.subs.append(commands)
        return j

    def heredoc(self, i):
        """Read the delimiter of a heredoc whose `<<` ends before `i`; its body starts at the next unquoted newline.

        The third `<` of a here-string ends the delimiter at once, so `<<<` opens nothing.
        """
        s = self.s
        strip_tabs = s.startswith("-", i)
        i += strip_tabs
        while i < len(s) and s[i] in " \t":
            i += 1
        delimiter = ""
        while i < len(s) and s[i] not in " \t\n;|&<>()":
            if s[i] in "'\"":
                j = s.find(s[i], i + 1)
                j = len(s) if j < 0 else j
                delimiter += s[i + 1:j]
                i = j + 1
                continue
            if s[i] == "\\" and i + 1 < len(s):
                i += 1
            delimiter += s[i]
            i += 1
        if delimiter:
            self.pending.append((delimiter, strip_tabs))
        return i

    def heredoc_bodies(self, i):
        """Skip the bodies of the heredocs opened on the line that ended before `i`."""
        s = self.s
        for delimiter, strip_tabs in self.pending:
            while i < len(s):
                j = s.find("\n", i)
                j = len(s) if j < 0 else j
                line = s[i:j]
                i = j + 1
                if (line.lstrip("\t") if strip_tabs else line) == delimiter:
                    break
        self.pending = []
        return min(i, len(s))


def commands_of(command):
    """Split a command line into simple commands, each a list of Words.

    `;`, `&&`, `||`, `|`, `&`, newlines and parentheses start a new command; a
    substitution's commands hang off the word that holds it. Redirection
    targets are kept as words beginning with `>`.
    """
    return Parser(command).parse(0)[0]


def name_and_args(words):
    """The command's name and its arguments, past assignments, wrappers like `env -C dir` and reserved words."""
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
            takes_argument = WRAPPERS[text]
            i += 1
            while i < len(words) and words[i].text.startswith("-"):
                i += 2 if words[i].text in takes_argument else 1
            continue
        if text in KEYWORDS:
            i += 1
            continue
        break
    if i >= len(words):
        return None, []
    return words[i].text.rsplit("/", 1)[-1], words[i + 1:]


def script_of(name, args):
    """The command line an `eval` or `sh -c` runs, or None."""
    if name == "eval":
        return " ".join(a.text for a in args)
    if name in SHELLS:
        for k, a in enumerate(args):
            if re.match(r"^-[a-zA-Z]*c[a-zA-Z]*$", a.text):
                operands = [b.text for b in args[k + 1:] if not b.text.startswith(("-", "+"))]
                return operands[0] if operands else None
    return None


class Simple:
    """One simple command: its words, name and arguments, and how many commands its own words hold."""

    def __init__(self, words, name, args, own, counts=True):
        self.words, self.name, self.args, self.own, self.counts = words, name, args, own, counts


def simple_commands(commands, depth=0):
    """Every simple command, those inside substitutions, `eval` and `sh -c` strings included.

    An `eval` or `sh -c` stands for the commands of its string: it is kept for
    its redirections, but does not count as a command of its own.
    """
    out = []
    for words in commands:
        name, args = name_and_args(words)
        script = script_of(name, args) if depth < MAX_DEPTH else None
        if script is not None:
            out.append(Simple(words, name, args, 0, counts=False))
            out += simple_commands(commands_of(script), depth + 1)
            continue
        nested = [x for w in words for sub in w.subs for x in simple_commands(sub, depth + 1)]
        out.append(Simple(words, name, args, sum(x.counts for x in nested)))
        out += nested
    return out


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
    simples = simple_commands(commands_of(command))
    total = sum(x.counts for x in simples)
    for x in simples:
        name, args, words = x.name, x.args, x.words
        # A substitution in cd's own argument (`cd "$(git rev-parse --show-toplevel)"`) is no second command.
        if name in ("cd", "pushd") and total - 1 - x.own > 0:
            return ("`cd` chained into a compound command stops the run for a permission prompt. "
                    "Use absolute paths, `git -C <dir>`, `npm --prefix <dir>`, or `env -C <dir> <cmd>` "
                    "for anything that must run in a directory (cargo, just).")
        if name == "rm":
            for a in args:
                if not a.text.startswith("-") and a.globs:
                    return ("`rm` with a glob stops the run for a permission prompt "
                            f"(`{a.text}`). Name each file, or leave them.")
        writes = name in WRITERS or (name in IN_PLACE and any(re.match(IN_PLACE[name], a.text) for a in args))
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
