#!/usr/bin/env bash
# The Bash guard's own test, run by `just witness-unit`: each shape CLAUDE.md's
# Machine section rules out is refused (exit 2, a message on stderr), and the
# commands beside it that look alike are let through. The pass cases are the
# control: a guard that refused everything would fail them.
# shellcheck disable=SC2016  # every $ below is command text handed to the guard, never expanded
set -euo pipefail
here=$(dirname "$0")
guard=$here/bash-guard.py

failures=0
check() { # check <pass|refuse> <command>
    local want=$1 command=$2 got=pass status=0
    python3 -I -c 'import json, sys; print(json.dumps({"tool_name": "Bash", "tool_input": {"command": sys.argv[1]}}))' "$command" |
        python3 -I "$guard" >/dev/null 2>&1 || status=$?
    [ "$status" -eq 0 ] || got=refuse
    [ "$status" -eq 0 ] || [ "$status" -eq 2 ] || got="exit $status"
    if [ "$got" != "$want" ]; then echo "FAIL: wanted $want, got $got: $command" >&2; failures=$((failures + 1)); fi
}

# cd
check refuse 'cd /tmp && ls'
check refuse 'cd /tmp; ls'
check refuse '(cd app && npm test)'
check refuse 'root=$(cd "$(dirname "$0")" && pwd)'
check refuse $'cd app\nnpm test'
check pass 'cd /Users/dev/Projects/knobas'
check pass 'git -C /Users/dev/Projects/knobas status --short'
check pass 'env -C /Users/dev/Projects/knobas just check'
check pass 'git commit -m "cd into the app && run the tests"'
check pass $'python3 -I - <<\'EOF\'\nimport os\nos.system("cd /tmp && ls")\nEOF'
check pass $'cat > /tmp/x.sh <<EOF\ncd "$(dirname "$0")" && pwd\nEOF'

# rm
check refuse 'rm -f $T/*.png'
check pass 'rm -f "$T/a.png"'
check refuse 'rm /tmp/shots/*.png'
check pass 'ls; rm -rf $W/target'
check refuse 'rm -f "$D"/shots/*.png'
check pass 'rm /Users/dev/Downloads/audit-2026-10-20.md'
check pass "rm -f '/tmp/odd \$name.png'"
check pass 'grep -rn "rm -f \$T/*" scripts'

# .git and ~/.claude
check refuse 'rm -rf .git/hooks'
check refuse 'echo x > .git/info/exclude'
check refuse 'mv a /Users/dev/Projects/knobas/.git/x'
check refuse 'cp settings.json ~/.claude/settings.json'
check refuse 'touch $HOME/.claude/x'
check refuse "sed -i '' s/a/b/ ~/.claude/settings.json"
check pass 'git -C /Users/dev/Projects/knobas worktree remove .worktrees/x'
check pass 'cat ~/.claude/settings.json'
check pass 'cp /tmp/a /Users/dev/Projects/knobas/.claude/commands/x.md'
check pass 'ls -la .git'
check pass 'just check > /Users/dev/.claude/jobs/90bee8a9/tmp/gate.log 2>&1'
check refuse 'echo x > /Users/dev/.claude/jobs/90bee8a9/settings.json'

# shell configuration
check refuse 'echo export X=1 >> ~/.bashrc'
check refuse 'cp /tmp/zshrc ~/.zshrc'
check pass 'grep PATH ~/.bashrc'

# not a Bash call, or no command: let through
printf '%s' '{"tool_name": "Read", "tool_input": {"file_path": "/tmp/x"}}' | python3 -I "$guard" || { echo "FAIL: a Read call was refused" >&2; failures=$((failures + 1)); }
printf '%s' 'not json' | python3 -I "$guard" || { echo "FAIL: unreadable input was refused" >&2; failures=$((failures + 1)); }

if [ "$failures" -gt 0 ]; then echo "bash-guard-test: $failures failure(s)" >&2; exit 1; fi
echo "bash-guard-test: ok"
