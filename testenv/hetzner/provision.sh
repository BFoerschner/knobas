#!/usr/bin/env bash
# Provision the three Hetzner servers that host the opt-in real products, one
# product per server, and wire the notebook to them:
#
#   knobas-teamcity     teamcity + teamcity-agent    (profile real-teamcity)
#   knobas-jira         jira + jira-db               (profile real-atlassian)
#   knobas-confluence   confluence + confluence-db   (profile real-atlassian)
#
# One per server because a cx23 has 4 GB, which holds one product's JVM and its
# database, and not two JVMs. The compose file is unchanged: `docker compose`
# runs here on the notebook against a docker context that dials the server over
# SSH, so the containers still bind 127.0.0.1 -- on the server -- and the
# notebook reaches them through `./tunnel up`, on the same ports as before.
#
# IDEMPOTENT: every step is find-then-create, so re-running after a partial run,
# or after deleting one server in the console, converges. It creates:
#   ~/.ssh/knobas-hetzner(.pub)   a dedicated key. The keys already in ~/.ssh are
#                                 FIDO hardware keys that want a touch per
#                                 connection, which docker-over-ssh makes dozens
#                                 of per compose call.
#   an hcloud ssh-key, a firewall (inbound: 22/tcp and ICMP only), 3 servers
#   a Host block per server in ~/.ssh/config (between marker comments)
#   hosts.env                     the servers' addresses (gitignored)
#   docker contexts knobas-teamcity, knobas-jira, knobas-confluence
#
# The token is HETZNER_API_TOKEN in the repo-root .env, exported as HCLOUD_TOKEN,
# which hcloud prefers over its active context -- so an `hcloud context` for some
# other project on this machine is never the one written to.
#
#   ./provision.sh            create or converge everything, then wait for docker
#   ./provision.sh --destroy  delete the three servers (volumes and data with them)
set -euo pipefail
cd "$(dirname "$0")"
ROOT=$(cd ../.. && pwd)

say() { echo "provision: $*"; }
die() { echo "provision: $*" >&2; exit 1; }

for tool in hcloud ssh ssh-keygen docker jq; do
  command -v "$tool" >/dev/null || die "$tool is required"
done
[ -r "$ROOT/.env" ] || die "$ROOT/.env not found; it carries HETZNER_API_TOKEN"
HCLOUD_TOKEN=$(sed -n 's/^HETZNER_API_TOKEN=//p' "$ROOT/.env" | tr -d '"'"'"' ')
[ -n "$HCLOUD_TOKEN" ] || die "no HETZNER_API_TOKEN in $ROOT/.env"
export HCLOUD_TOKEN
unset HCLOUD_CONTEXT

ROLES="teamcity jira confluence"
# cpx22 (2 shared AMD vCPU, 4 GB, ~EUR 23/month) for the two Atlassian products
# since 2026-09-06. The history: on a cx23 (2 Intel vCPU) Jira took 147 s to
# FIRST_RUN against 56 s on the notebook; a cx33 (4 Intel vCPU) took 174 s --
# no faster, zero steal -- because Jira's start is single-threaded and the cx
# line is a 2.1 GHz Skylake. The AMD line is the lever left for that; cpx22 is
# its cheapest member still offered (cpx21/cpx31 are legacy: "unsupported
# location" in every EU location). The pair lives between runs
# (renew-licence.sh), so it is 4 GB per product with the swap as margin, as the
# cx23 already carried. TeamCity stays on a cx23: it is started once and left
# running. An existing server whose type differs is resized in place below
# (powered off, changed, powered on; disk kept at 40 GB so the change stays
# reversible).
type_of() {
  case "$1" in
    teamcity) echo "${KNOBAS_HETZNER_TYPE_TEAMCITY:-cx23}" ;;
    *)        echo "${KNOBAS_HETZNER_TYPE_ATLASSIAN:-cpx22}" ;;
  esac
}
LOCATION=${KNOBAS_HETZNER_LOCATION:-nbg1}
IMAGE=ubuntu-24.04
KEY=$HOME/.ssh/knobas-hetzner
KEY_NAME=knobas-hetzner
FW=knobas-testenv
SSH_CONFIG=$HOME/.ssh/config
MARK_BEGIN='# >>> knobas testenv/hetzner (provision.sh writes this block) >>>'
MARK_END='# <<< knobas testenv/hetzner <<<'

if [ "${1:-}" = "--destroy" ]; then
  for role in $ROLES; do
    if hcloud server describe "knobas-$role" >/dev/null 2>&1; then
      say "deleting knobas-$role"; hcloud server delete "knobas-$role" >/dev/null
    fi
    docker context rm -f "knobas-$role" >/dev/null 2>&1 || true
  done
  rm -f hosts.env
  say "servers gone; the firewall, the ssh key and the ~/.ssh/config block stay"
  exit 0
fi
[ $# -eq 0 ] || die "unknown argument '$1'; usage: ./provision.sh [--destroy]"

# 1. The key ----------------------------------------------------------------
if [ ! -r "$KEY" ]; then
  say "generating $KEY"
  ssh-keygen -q -t ed25519 -N '' -C 'knobas testenv hetzner' -f "$KEY"
fi
if ! hcloud ssh-key describe "$KEY_NAME" >/dev/null 2>&1; then
  say "uploading ssh key $KEY_NAME"
  hcloud ssh-key create --name "$KEY_NAME" --public-key-from-file "$KEY.pub" --label knobas=testenv >/dev/null
fi

# 2. The firewall -----------------------------------------------------------
if ! hcloud firewall describe "$FW" >/dev/null 2>&1; then
  say "creating firewall $FW (inbound 22/tcp + icmp)"
  hcloud firewall create --name "$FW" --rules-file firewall-rules.json --label knobas=testenv >/dev/null
fi

# 3. The servers ------------------------------------------------------------
for role in $ROLES; do
  name=knobas-$role
  type=$(type_of "$role")
  if hcloud server describe "$name" >/dev/null 2>&1; then
    have=$(hcloud server describe "$name" -o json | jq -r '.server_type.name')
    if [ "$have" = "$type" ]; then
      say "$name exists ($type)"
    else
      say "$name is a $have; resizing to $type (powered off for a minute)"
      [ "$(hcloud server describe "$name" -o json | jq -r .status)" = off ] || hcloud server poweroff "$name" >/dev/null
      hcloud server change-type "$name" "$type" >/dev/null
      hcloud server poweron "$name" >/dev/null
    fi
  else
    say "creating $name ($type, $LOCATION, $IMAGE)"
    hcloud server create --name "$name" --type "$type" --image "$IMAGE" --location "$LOCATION" \
      --ssh-key "$KEY_NAME" --firewall "$FW" --user-data-from-file cloud-init.yml \
      --label knobas=testenv --label role="$role" >/dev/null
  fi
done

# 4. Addresses, ssh config, contexts -----------------------------------------
: > hosts.env.tmp
block=$(mktemp)
echo "$MARK_BEGIN" >> "$block"
for role in $ROLES; do
  ip=$(hcloud server ip "knobas-$role")
  [ -n "$ip" ] || die "knobas-$role has no IPv4"
  echo "KNOBAS_HETZNER_$(tr a-z A-Z <<<"$role")_IP=$ip" >> hosts.env.tmp
  cat >> "$block" <<BLOCK
Host knobas-$role
  HostName $ip
  User root
  IdentityFile $KEY
  IdentitiesOnly yes
  UserKnownHostsFile ~/.ssh/knobas-hetzner.known_hosts
  StrictHostKeyChecking accept-new
  ServerAliveInterval 30
  ServerAliveCountMax 3
  ControlMaster auto
  ControlPath ~/.ssh/cm-knobas-%r@%h:%p
  ControlPersist 10m
BLOCK
done
echo "$MARK_END" >> "$block"
mv hosts.env.tmp hosts.env
say "wrote hosts.env:"; sed 's/^/  /' hosts.env

# Replace our block in ~/.ssh/config, or append it. It goes at the END: an
# OrbStack `Include` sits at the top, and a Host block of ours before it would
# leave the include reading under a non-matching Host.
touch "$SSH_CONFIG"; chmod 600 "$SSH_CONFIG"
if grep -qF "$MARK_BEGIN" "$SSH_CONFIG"; then
  awk -v b="$MARK_BEGIN" -v e="$MARK_END" '$0==b{skip=1} !skip{print} $0==e{skip=0}' "$SSH_CONFIG" > "$SSH_CONFIG.tmp"
  # awk drops a trailing blank line before our block; keep exactly one.
  printf '%s\n' "$(cat "$SSH_CONFIG.tmp")" > "$SSH_CONFIG.tmp"
  { cat "$SSH_CONFIG.tmp"; echo; cat "$block"; } > "$SSH_CONFIG"; rm -f "$SSH_CONFIG.tmp"
else
  { echo; cat "$block"; } >> "$SSH_CONFIG"
fi
rm -f "$block"
say "ssh config: Host knobas-teamcity / knobas-jira / knobas-confluence"

for role in $ROLES; do
  if docker context inspect "knobas-$role" >/dev/null 2>&1; then
    docker context update "knobas-$role" --docker "host=ssh://knobas-$role" >/dev/null
  else
    docker context create "knobas-$role" --docker "host=ssh://knobas-$role" \
      --description "knobas testenv: $role on Hetzner" >/dev/null
  fi
done

# 5. Wait for first boot to finish and docker to answer ------------------------
for role in $ROLES; do
  printf 'provision: waiting for knobas-%s ssh ' "$role"
  i=0
  until ssh -o ConnectTimeout=5 -o BatchMode=yes "knobas-$role" true 2>/dev/null; do
    i=$((i + 1)); [ "$i" -lt 60 ] || { echo; die "knobas-$role: no ssh after 300s"; }
    printf '.'; sleep 5
  done
  echo ' ok'
  say "knobas-$role: waiting for cloud-init (docker install)"
  ssh "knobas-$role" 'cloud-init status --wait >/dev/null; cloud-init status --long | sed -n "1,3p"; test -e /var/lib/knobas-provisioned' \
    || die "knobas-$role: cloud-init did not finish cleanly; ssh knobas-$role journalctl -u cloud-final"
  docker --context "knobas-$role" info --format 'provision: knobas-'"$role"': docker {{.ServerVersion}}, {{.NCPU}} cpu, {{.MemTotal}} bytes ram' \
    || die "knobas-$role: docker context does not answer"
done
say "done. Next: ./tunnel up, then the compose/seed commands from README.md with 'source env' in the shell"
