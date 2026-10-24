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
fail() { echo "FAIL: $1" >&2; failures=$((failures + 1)); }
call() { # call <command>: the Bash tool call a hook reads on stdin
    python3 -I -c 'import json, sys; print(json.dumps({"tool_name": "Bash", "tool_input": {"command": sys.argv[1]}}))' "$1"
}
check() { # check <pass|refuse> <command>
    local want=$1 command=$2 got=pass status=0
    call "$command" | python3 -I "$guard" >/dev/null 2>&1 || status=$?
    [ "$status" -eq 0 ] || got=refuse
    [ "$status" -eq 0 ] || [ "$status" -eq 2 ] || got="exit $status"
    [ "$got" = "$want" ] || fail "wanted $want, got $got: $command"
}
says() { # says <the start of a rule's message> <command>: refused with that rule's message
    local want=$1 command=$2 err status=0
    err=$(call "$command" | python3 -I "$guard" 2>&1 >/dev/null) || status=$?
    case "$status $err" in "2 bash-guard: $want"*) ;; *) fail "wanted \"$want\", got $status $err: $command" ;; esac
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
# compound shapes: keywords, braces and `builtin` are not the command's name
check refuse '{ cd /tmp && ls; }'
check refuse 'if cd /tmp; then ls; fi'
check refuse 'for d in a b; do cd $d && ls; done'
check refuse $'for d in a b\ndo\n  cd $d\ndone'
check refuse 'while true; do cd x; done'
check refuse '! cd x || ls'
check refuse 'builtin cd /tmp && ls'
check pass '{ ls; }'
check pass 'if test -d x; then ls; fi'
check pass 'for d in a b; do echo $d; done'
check pass 'echo builtin'
check pass 'builtin echo hi && ls'
check pass 'command -v cd >/dev/null && echo yes'
check refuse 'command cd /tmp && ls'
check pass 'git commit -m "{ cd x && y; }"'
# eval and shell strings are parsed as the commands they run
check refuse 'eval "cd x && y"'
check refuse 'bash -c "cd x && y"'
check refuse "sh -c 'cd x; y'"
check refuse 'bash -ec "cd x && y"'
check refuse 'xargs -I{} sh -c "cd {} && ls"'
check refuse 'bash -c "echo x > .git/info/exclude"'
check pass 'bash -c "ls && pwd"'
check pass 'eval "ls; pwd"'
check pass 'bash -c "cd /tmp"'
check pass 'bash scripts/bash-guard-test.sh'
check pass 'echo "bash -c cd x && y"'
# a substitution is a second command, except in cd's own argument
check pass 'cd "$(git rev-parse --show-toplevel)"'
check pass 'cd $(git rev-parse --show-toplevel)'
check pass 'cd `pwd`'
check refuse 'cd $(pwd) && ls'
check refuse 'cd "$(cd x && pwd)"'
check refuse 'echo "$(cd /tmp && pwd)"'
check refuse 'echo "`cd /tmp && pwd`"'
# testenv/README.md names the passing form of the refused one
check refuse 'eval "$(cd testenv && ./seed --env)"'
check pass 'eval "$(env -C testenv ./seed --env)"'
check pass 'ls "$(git rev-parse --show-toplevel)"'
check pass $'git commit -m "$(cat <<\'EOF\'\nParse heredocs; cd x && y is data\nEOF\n)"'
# a comment is not a command
check pass 'ls # then cd x && y'
check pass 'echo a#b'

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

# heredocs: only an unquoted `<<` that is not `<<<` opens one, so a phantom never hides a later line
check refuse $'echo "<<EOF"\necho x > .git/info/exclude'
check refuse $'cat <<<EOF\necho x > .git/info/exclude'
check refuse $'grep -rn \'<<EOF\' scripts\necho x > .git/info/exclude'
check refuse $'echo $((1<<2))\necho x > .git/info/exclude'
check refuse $'cat > /tmp/x <<EOF\nbody\nEOF\necho x > .git/info/exclude'
check refuse $'cat > /tmp/x <<"EOF"\nbody\nEOF\necho x > .git/info/exclude'
check pass $'echo "<<EOF"; echo "it\'s\nfine"\nEOF\necho "> .git/x"'
check pass $'cat > /tmp/x <<EOF\necho x > .git/info/exclude\nEOF'
check pass $'cat <<-EOF\n\tcd x && y\n\tEOF\nls'
check pass $'cat <<A <<B\ncd x && y\nA\ncd x && y\nB'
check pass 'cat <<<"$x" > /tmp/y'
# a bare `((...))` is arithmetic: its `<<` is a shift, so the next line is read
check refuse $'(( n = 1 << 3 ))\ncd x && ls'
check refuse $'((x<<2))\necho x > .git/info/exclude'
check refuse $'if ((x<<2)); then ls; fi\ncd x && ls'
check refuse $'for ((i=0; i<<1; i++)); do ls; done\ncd x && ls'
check refuse $'time -p ((x<<2))\ncd x && ls'
check refuse $'coproc ((x<<2))\ncd x && ls'
check pass '((x<<2))'
check pass 'for ((i=0; i<3; i++)); do echo $i; done'
# ...but a `((` whose inner group is not closed by `))` is two subshells, as the shell reads it
check refuse '((cd x); ls)'
check refuse 'echo $((cd x); ls)'
# ...and a heredoc opened in it is read once, not again for each reading
check refuse $'echo $(( $(cat <<EOF) ); ls)\nbody\nEOF\ncd x && y'
check refuse $'(( $(cat <<EOF) ); ls)\nbody\nEOF\ncd x && y'
# a substitution inside arithmetic runs
check refuse 'echo $(( $(cd x && pwd) + 1 ))'
check refuse '(( $(cd x && echo 1) ))'
check pass 'echo $(( $(wc -l < x) + 1 ))'
# `function f` is no command: the body's first word is
check refuse 'function f { cd x; ls; }'
check pass 'function f { ls; }'
# the shell runs the substitutions in the body of a heredoc whose delimiter is unquoted
check refuse $'cat <<EOF\n$(cd x && pwd)\nEOF'
check refuse $'cat <<EOF\n`cd x && pwd`\nEOF'
check refuse $'cat > /tmp/x <<EOF\nline $(cd x &&\npwd) end\nEOF'
check refuse $'<<EOF\n$(cd x && pwd)\nEOF\nls'
check refuse $'<<EOF\n$(pwd)\nEOF\ncd x'
check pass $'cat <<\'EOF\'\n$(cd x && pwd)\nEOF'
check pass $'cat <<"EOF"\n`cd x && pwd`\nEOF'
check pass $'cat <<\\EOF\n$(cd x && pwd)\nEOF'
check pass $'cat <<EOF\n\\$(cd x && pwd)\nEOF'
check pass $'cat <<EOF\nroot: $(git rev-parse --show-toplevel)\nEOF'
check pass $'cat > /tmp/y <<EOF\n$(cat ./.git/HEAD)\nEOF'

# a redirection before the name is no name: the word after it is (#1981)
cd_rule='`cd` chained' rm_rule='`rm` with a glob' write_rule='A Bash write under'
says "$cd_rule" '>/dev/null cd /tmp && ls'
says "$cd_rule" '2>/dev/null cd /tmp && ls'
says "$cd_rule" '</dev/null cd /tmp && ls'
says "$cd_rule" '{ >/dev/null cd /tmp; ls; }'
says "$cd_rule" 'if >/dev/null cd /tmp; then ls; fi'
says "$cd_rule" '&>/dev/null cd /tmp && ls'
says "$rm_rule" '>/dev/null rm -f *.png'
says "$write_rule" '2>&1 sed -i s/a/b/ ~/.zshrc'
says "$write_rule" '</dev/null cp x ~/.claude/settings.json'
says "$write_rule" '2> .git/x ls'
says "$cd_rule" 'cd /tmp 2>/dev/null && ls'
check pass 'cd /tmp 2>/dev/null'
check pass '>/dev/null cd /tmp'
check pass 'ls &>/dev/null'
check pass 'cd /tmp &>/dev/null'
check pass 'exec >/dev/null 2>&1'
check pass 'diff <(ls a) <(ls b) 2>&1'
check pass "echo '>.git/x'"

# a `<<` inside `$[...]` or `${...}` is a shift or pattern text, no heredoc, so the next line is read
says "$cd_rule" $'echo $[1<<2]\ncd /tmp && ls'
says "$cd_rule" $'echo ${x/<<EOF/y}\ncd /tmp && ls'
says "$cd_rule" $'echo ${x:-"}<<EOF"}\ncd /tmp && ls'
says "$cd_rule" 'echo ${x:-$(cd /tmp && ls)}'
says "$rm_rule" 'rm -f ${T}/*.png'
check pass 'echo ${x/<<EOF/y} $[1<<2]'
check pass 'echo ${#x} # cd x && y'

# a script a shell reads on stdin, a command `find -exec` runs, and a `case` in a substitution are read
says "$cd_rule" $'bash <<\'EOF\'\ncd /tmp && ls\nEOF'
says "$cd_rule" $'sh -s x <<EOF\ncd /tmp\nls\nEOF'
says "$cd_rule" "bash <<<'cd /tmp && ls'"
says "$write_rule" 'bash <<< "echo x > .git/info/exclude"'
says "$cd_rule" "find . -exec sh -c 'cd /tmp && ls' _ {} \\;"
says "$rm_rule" 'find . -type d -execdir rm -f *.png +'
says "$cd_rule" 'echo $(case a in a) cd /tmp && ls;; esac)'
says "$cd_rule" 'echo $(case a in (a|b) ls;; c) cd /tmp && ls;; esac)'
check pass $'bash scripts/x.sh <<\'EOF\'\ncd /tmp && ls\nEOF'
check pass $'cat <<\'EOF\' | grep x\ncd /tmp && ls\nEOF'
check pass "bash <<<'ls; pwd'"
check pass 'find . -name "*.png" -exec ls {} + -print'
check pass 'echo $(case $x in a) echo a;; esac) && ls'

# shell configuration
check refuse 'echo export X=1 >> ~/.bashrc'
check refuse 'cp /tmp/zshrc ~/.zshrc'
check pass 'grep PATH ~/.bashrc'
check refuse 'perl -pi -e s/a/b/ ~/.zshrc'
check refuse 'perl -i -pe s/a/b/ ~/.zshrc'
check refuse 'perl -pi.bak -e s/a/b/ ~/.zshrc'
check refuse 'sed -Ei s/a/b/ ~/.bashrc'
check refuse 'sed --in-place=.bak s/a/b/ ~/.bashrc'
check pass 'perl -pe s/a/b/ ~/.zshrc'
check pass 'sed -n /PATH/p ~/.bashrc'
# an `i` in an option's glued argument is no in-place flag (`-Mstrict`, `-Ilib`, `-fedit.sed`)
check pass "perl -Mstrict -ne 'print' ~/.bashrc"
check pass "perl -MList::Util=sum -ne 'print' ~/.bashrc"
check pass 'perl -Ilib -ne print ~/.bashrc'
check pass 'sed -fedit.sed ~/.bashrc'
check refuse 'perl -lni -e print ~/.zshrc'
check refuse 'perl -0777 -pi -e s/a/b/ ~/.zshrc'
check refuse 'sed -ni /x/p ~/.bashrc'
check refuse "sed -I '' s/a/b/ ~/.bashrc"
check refuse 'sed --in-pl s/a/b/ ~/.bashrc'

# not a Bash call, or no command: let through
printf '%s' '{"tool_name": "Read", "tool_input": {"file_path": "/tmp/x"}}' | python3 -I "$guard" || fail "a Read call was refused"
printf '%s' 'not json' | python3 -I "$guard" || fail "unreadable input was refused"

# the wiring in .claude/settings.json: a refusal comes through as exit 2, and a
# missing script lets the call through (any status but 2) and says why on stderr
hook=$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["hooks"]["PreToolUse"][0]["hooks"][0]["command"])' "$here/../.claude/settings.json")
wired() { # wired <project dir> <command>: the hook's exit status, a space, its stderr
    local status=0 err
    err=$(call "$2" | CLAUDE_PROJECT_DIR=$1 sh -c "$hook" 2>&1 >/dev/null) || status=$?
    echo "$status $err"
}
case $(wired "$here/.." 'cd /tmp && ls') in "2 bash-guard: \`cd\` chained"*) ;; *) fail "the wired hook did not refuse a chained cd" ;; esac
case $(wired "$here/.." 'ls') in "0 ") ;; *) fail "the wired hook refused ls" ;; esac
missing=$(wired /nonexistent/bash-guard-test 'cd /tmp && ls')
case $missing in
    "2 "*) fail "a missing script blocked the call: $missing" ;;
    *" bash-guard: /nonexistent/bash-guard-test/scripts/bash-guard.py is missing"*) ;;
    *) fail "a missing script did not say why: $missing" ;;
esac

if [ "$failures" -gt 0 ]; then echo "bash-guard-test: $failures failure(s)" >&2; exit 1; fi
echo "bash-guard-test: ok"
