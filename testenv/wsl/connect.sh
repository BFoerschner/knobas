#!/usr/bin/env bash
# Point this Mac's live suites at a WSL box set up with setup-wsl.sh.
#
#   testenv/wsl/connect.sh <address> [port] [user]     # port 2222, user $USER
#   testenv/wsl/connect.sh --local                     # back to this machine
#
# 1. ~/.ssh/knobas-wsl, an ed25519 key with no passphrase, made if missing: the
#    live runs are unattended, and a security key wants a touch per login.
#    Its public half is printed for setup-wsl.sh --key.
# 2. A `Host knobas-wsl` block in ~/.ssh/config between marker lines, replaced
#    on a re-run, with a ControlMaster: every docker call is an ssh login.
# 3. testenv/live.env in the MAIN checkout (every worktree reads that one):
#    KNOBAS_LIVE_TARGET=remote, KNOBAS_LIVE_SSH=knobas-wsl.
# 4. A docker context `wsl` (ssh://knobas-wsl), made if missing: the context
#    testenv/estate.json's engine names, which `just estate-live` reads.
# 5. testenv/live remote-check: ssh, the engine, its memory, the tunnel.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)
envfile="${common%/.git}/testenv/live.env"

if [ "${1:-}" = --local ]; then
  if [ -f "$envfile" ]; then rm -f "$envfile"; echo "connect: removed $envfile; the live products run on this machine again"; fi
  "$here/../live" tunnel down 2>/dev/null || true
  exit 0
fi
[ $# -ge 1 ] && [ $# -le 3 ] || { echo "usage: connect.sh <address> [port] [user] | --local" >&2; exit 2; }
addr=$1 port=${2:-2222} user=${3:-$USER}
alias=knobas-wsl key=$HOME/.ssh/knobas-wsl conf=$HOME/.ssh/config

if [ ! -f "$key" ]; then
  ssh-keygen -q -t ed25519 -N '' -C "knobas-wsl $(hostname -s)" -f "$key"
  echo "connect: made $key"
fi

begin="# >>> knobas testenv/wsl (connect.sh writes this block) >>>"
end="# <<< knobas testenv/wsl <<<"
touch "$conf"
tmp=$(mktemp)
awk -v b="$begin" -v e="$end" '$0 == b { skip = 1 } !skip { print } $0 == e { skip = 0 }' "$conf" >"$tmp"
cat >>"$tmp" <<EOF
$begin
Host $alias
  HostName $addr
  Port $port
  User $user
  IdentityFile $key
  IdentitiesOnly yes
  StrictHostKeyChecking accept-new
  ServerAliveInterval 30
  ServerAliveCountMax 3
  ControlMaster auto
  ControlPath ~/.ssh/cm-$alias-%r@%h:%p
  ControlPersist 10m
$end
EOF
cat "$tmp" >"$conf"
rm -f "$tmp"
chmod 600 "$conf"
echo "connect: ~/.ssh/config has Host $alias -> $user@$addr:$port"

if ! ssh -o BatchMode=yes -o ConnectTimeout=10 "$alias" true 2>/dev/null; then
  echo "connect: cannot log in to $alias yet. Inside WSL run:" >&2
  echo "  bash setup-wsl.sh --port $port --key '$(cat "$key.pub")'" >&2
  echo "connect: then re-run this." >&2
  exit 1
fi

cat >"$envfile" <<EOF
# Written by testenv/wsl/connect.sh; testenv/live.env.example says what goes here.
KNOBAS_LIVE_TARGET=remote
KNOBAS_LIVE_SSH=$alias
EOF
echo "connect: wrote $envfile"
# The `wsl` context: what testenv/estate.json's engine names, and what the
# Docker importer and `just estate-live` read the box's containers through.
docker context inspect wsl >/dev/null 2>&1 || docker context create wsl --description "knobas live products on the WSL box" --docker host=ssh://knobas-wsl
"$here/../live" remote-check
