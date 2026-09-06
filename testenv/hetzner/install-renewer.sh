#!/usr/bin/env bash
# Install the licence renewer on the two Atlassian servers: renew-licence.sh and
# the fetcher it uses go to /opt/knobas, and a systemd timer runs the renewer
# every 150 minutes -- 30 minutes inside the 3-hour life of a timebomb key --
# and once at boot. Idempotent; re-run after editing either script.
#
#   ./install-renewer.sh              both servers
#   ./install-renewer.sh jira         one of them
#
# It runs ON THE SERVER rather than from the notebook because the notebook
# sleeps and the pair does not. `systemctl status knobas-renew-licence.timer`
# and `journalctl -u knobas-renew-licence` on the server say what it did;
# `./tunnel status` reads the status file it writes.
set -euo pipefail
cd "$(dirname "$0")"
say() { echo "install-renewer: $*"; }
die() { echo "install-renewer: $*" >&2; exit 1; }

roles=${1:-"jira confluence"}
for role in $roles; do
  case "$role" in jira|confluence) ;; *) die "no renewer for '$role'; jira or confluence" ;; esac
  host=knobas-$role
  say "$host: copying scripts"
  ssh "$host" 'mkdir -p /opt/knobas'
  scp -q renew-licence.sh ../fetch-timebomb-keys.sh "$host:/opt/knobas/"
  ssh "$host" "chmod +x /opt/knobas/renew-licence.sh /opt/knobas/fetch-timebomb-keys.sh
    cat > /etc/systemd/system/knobas-renew-licence.service <<UNIT
[Unit]
Description=knobas: re-apply the $role timebomb licence
After=docker.service network-online.target
Wants=network-online.target

[Service]
Type=oneshot
Environment=PRODUCT=$role
ExecStart=/opt/knobas/renew-licence.sh
UNIT
    cat > /etc/systemd/system/knobas-renew-licence.timer <<UNIT
[Unit]
Description=knobas: renew the $role timebomb licence every 150 minutes

[Timer]
OnBootSec=3min
OnUnitActiveSec=150min
AccuracySec=1min
Persistent=true

[Install]
WantedBy=timers.target
UNIT
    systemctl daemon-reload
    systemctl enable --now knobas-renew-licence.timer >/dev/null 2>&1
    # OnUnitActiveSec counts from the service's last activation, and a timer
    # just enabled has none, so the first renewal is started here by hand --
    # which restarts the product once, now.
    systemctl start knobas-renew-licence.service
    cat /opt/knobas/renew-status
    systemctl list-timers knobas-renew-licence.timer --no-pager | sed -n 1,2p"
done
say "done"
