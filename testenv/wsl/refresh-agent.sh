#!/usr/bin/env bash
# The launchd agent that keeps a remote target's Jira and Confluence licensed
# between runs (#2373): `testenv/live refresh` from the MAIN checkout, every
# 15 minutes and at login. It runs on the Mac, not the box, because the
# per-product locks it must respect live in this Mac's $TMPDIR
# (testenv/README.md, "Live products on a remote machine (WSL)").
#
#   testenv/wsl/refresh-agent.sh install     write the plist and load it
#   testenv/wsl/refresh-agent.sh uninstall   unload it and remove the plist
#   testenv/wsl/refresh-agent.sh status      loaded or not, and the log's tail
#
# launchd gives an agent a bare PATH and may give it another TMPDIR, so the
# plist carries this shell's: docker, jq and ssh must resolve, and the locks
# must be the ones every live run takes.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)
main=${common%/.git}
label=com.knobas.live-refresh
plist=$HOME/Library/LaunchAgents/$label.plist
log=$HOME/Library/Logs/knobas-live-refresh.log
domain=gui/$(id -u)
interval=${KNOBAS_LIVE_REFRESH_EVERY_S:-900}

# xml <text> -- escaped for a plist string.
xml() { printf '%s' "$1" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'; }

case "${1:-}" in
  install)
    case $interval in '' | 0* | *[!0-9]*)
      echo "refresh-agent: KNOBAS_LIVE_REFRESH_EVERY_S must be whole seconds, not '$interval'" >&2; exit 1 ;;
    esac
    mkdir -p "$(dirname "$plist")" "$(dirname "$log")"
    cat >"$plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$label</string>
  <key>ProgramArguments</key>
  <array><string>$(xml "$(command -v bash)")</string><string>$(xml "$main/testenv/live")</string><string>refresh</string></array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>$(xml "$PATH")</string>
    <key>TMPDIR</key><string>$(xml "${TMPDIR:-/tmp}")</string>
    <key>HOME</key><string>$(xml "$HOME")</string>
  </dict>
  <key>StartInterval</key><integer>$interval</integer>
  <key>RunAtLoad</key><true/>
  <key>StandardOutPath</key><string>$(xml "$log")</string>
  <key>StandardErrorPath</key><string>$(xml "$log")</string>
</dict>
</plist>
EOF
    plutil -lint "$plist" >/dev/null
    launchctl bootout "$domain/$label" 2>/dev/null || true
    launchctl bootstrap "$domain" "$plist"
    echo "refresh-agent: $label runs $main/testenv/live refresh every ${interval}s; log $log" ;;
  uninstall)
    launchctl bootout "$domain/$label" 2>/dev/null || true
    rm -f "$plist"
    echo "refresh-agent: $label removed" ;;
  status)
    if launchctl print "$domain/$label" >/dev/null 2>&1; then echo "refresh-agent: $label loaded"; else echo "refresh-agent: $label not loaded"; fi
    [ ! -f "$log" ] || tail -n 12 "$log" ;;
  *) echo "usage: refresh-agent.sh install|uninstall|status" >&2; exit 2 ;;
esac
