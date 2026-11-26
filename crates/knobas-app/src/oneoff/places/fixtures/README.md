# What real tools print

Captured on the WSL box (Ubuntu 24.04) on 2026-11-24 by running each tool on a small repository at
`/home/dev/runcmd/fixrepo`, with `FORCE_COLOR=1 CLICOLOR_FORCE=1 PY_COLORS=1 TERM=xterm-256color` set
as knobas sets them: ESLint 9.39 (stylish, with a relative and an absolute path), tsc 5.9
(`--pretty false`), Ruff 0.17 (concise and full), ShellCheck 0.10 (tty and `-f gcc`), xmllint of
libxml2 2.15 (a schema failure and a parser error), Python 3's traceback, and rustc (in `crate/`).
`places/tests.rs` reads each through the reader and the place finder and counts what it finds.
