#!/usr/bin/env bash
# Make a WSL distribution the always-on host of knobas' live products.
# Runs INSIDE WSL (Ubuntu or Debian), as the user the Mac will ssh in as:
#
#   bash setup-wsl.sh --key "ssh-ed25519 AAAA... knobas-wsl"   [--port 2222]
#
# What it does, each step skipped when already done:
#   1. systemd as PID 1 (/etc/wsl.conf), so dockerd and sshd start on boot.
#      The first run stops here when it had to switch it on: `wsl --shutdown`
#      from Windows, open the distribution again, re-run this.
#   2. Docker Engine (the distribution's docker.io) and openssh-server.
#   3. This user in the `docker` group: the Mac's DOCKER_HOST=ssh://... runs
#      `docker system dial-stdio` as this user.
#   4. sshd on --port (2222 by default, clear of a Windows OpenSSH on 22),
#      keys only, and --key in ~/.ssh/authorized_keys.
#
# What it does not do: anything on the Windows side (memory, keeping WSL
# running, the firewall) -- that is setup-windows.ps1 -- and no checkout: the
# Mac drives compose and the seeds, this machine only runs the engine. The
# products' ports stay bound to 127.0.0.1 here and reach the Mac through ssh.
# testenv/README.md, "Live products on a remote machine (WSL)".
set -euo pipefail

port=2222 key=""
while [ $# -gt 0 ]; do
  case $1 in
    --port) port=$2; shift 2 ;;
    --key) key=$2; shift 2 ;;
    *) echo "usage: setup-wsl.sh --key '<ssh public key>' [--port 2222]" >&2; exit 2 ;;
  esac
done
case $port in '' | *[!0-9]*) echo "setup-wsl: --port must be a number" >&2; exit 2 ;; esac

grep -qi microsoft /proc/version 2>/dev/null ||
  echo "setup-wsl: WARNING this does not look like WSL; going on anyway" >&2
[ "$(id -u)" != 0 ] || { echo "setup-wsl: run as the user the Mac will log in as, not root (it uses sudo)" >&2; exit 2; }
user=$(id -un)

# 1. systemd
if [ "$(ps -p 1 -o comm= 2>/dev/null)" != systemd ]; then
  if ! grep -qs '^systemd=true' /etc/wsl.conf; then
    printf '\n[boot]\nsystemd=true\n' | sudo tee -a /etc/wsl.conf >/dev/null
    echo "setup-wsl: switched systemd on in /etc/wsl.conf"
  fi
  echo "setup-wsl: systemd is not PID 1 yet. From Windows: 'wsl --shutdown', open the distribution again, re-run this." >&2
  exit 1
fi
echo "setup-wsl: systemd is PID 1"

# 2. packages
need=()
command -v docker >/dev/null || need+=(docker.io)
[ -x /usr/sbin/sshd ] || need+=(openssh-server)
if [ ${#need[@]} -gt 0 ]; then
  sudo apt-get update -q
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q "${need[@]}"
fi
if docker info --format '{{.OperatingSystem}}' 2>/dev/null | grep -qi 'docker desktop'; then
  echo "setup-wsl: WARNING docker here is Docker Desktop's engine, which runs only while Docker Desktop does;" >&2
  echo "setup-wsl:   for 24/7, turn off its WSL integration for this distribution and re-run (docker.io gets installed)" >&2
fi
sudo systemctl enable --now docker >/dev/null

# 3. docker group
if ! id -nG "$user" | tr ' ' '\n' | grep -qx docker; then
  sudo usermod -aG docker "$user"
  echo "setup-wsl: added $user to the docker group (a new login picks it up)"
fi

# 4. sshd
sudo tee /etc/ssh/sshd_config.d/knobas.conf >/dev/null <<EOF
# Written by knobas testenv/wsl/setup-wsl.sh
Port $port
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
EOF
if [ -n "$key" ]; then
  mkdir -p ~/.ssh && chmod 700 ~/.ssh
  touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys
  grep -qxF -- "$key" ~/.ssh/authorized_keys || { printf '%s\n' "$key" >>~/.ssh/authorized_keys; echo "setup-wsl: key added to ~/.ssh/authorized_keys"; }
elif [ ! -s ~/.ssh/authorized_keys ]; then
  echo "setup-wsl: WARNING no --key and no authorized_keys: nobody can log in (password login is off)" >&2
fi
# Ubuntu 24.04 starts sshd from ssh.socket, which ignores Port in sshd_config.
if systemctl list-unit-files ssh.socket >/dev/null 2>&1 && systemctl is-enabled ssh.socket >/dev/null 2>&1; then
  sudo systemctl disable --now ssh.socket >/dev/null 2>&1 || true
fi
sudo systemctl enable ssh >/dev/null 2>&1 || sudo systemctl enable sshd >/dev/null
sudo systemctl restart ssh 2>/dev/null || sudo systemctl restart sshd

mem_gb=$(awk '/MemTotal/ {printf "%d", $2 / 1024 / 1024}' /proc/meminfo)
# sudo: this login predates the docker group it may just have been added to.
engine=$(sudo docker version --format '{{.Server.Version}}')
echo "setup-wsl: done -- sshd on port $port, docker $engine, ${mem_gb} GB memory"
# The Mac's memory budget plans against this (live-lib.sh): about 9 GB holds
# any two JVM products beside Gitea, a third stops one of them.
[ "$mem_gb" -ge 8 ] ||
  echo "setup-wsl: WARNING under 8 GB: only one JVM product at a time will fit the budget -- memory= in %UserProfile%\\.wslconfig (setup-windows.ps1)" >&2
echo "setup-wsl: next, on the Mac: testenv/wsl/connect.sh <this machine's address> $port"
