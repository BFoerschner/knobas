#!/usr/bin/env bash
# Appends one merged PR's stage times to the loop's throughput log (Björn, 2026-10-15).
# Usage: .claude/loop/stages.sh <PR> <gate kind: check|check-front|none> <gate s> <batched with PR or -> <model> <agents running>
# Times come from GitHub: claim = the issue's last `in-progress` label event,
# draft = PR created, ready = last ready_for_review event (else created), merged.
# Columns: merged_at pr issue model claim→draft draft→ready ready→merged total(min) gate gate_s batched agents load
set -euo pipefail
pr=$1 kind=$2 secs=$3 batched=$4 model=$5 agents=$6
log=/Users/dev/knobas/.worktrees/.throughput.tsv
repo=BFoerschner/knobas
read -r created merged issue < <(gh pr view "$pr" --repo $repo --json createdAt,mergedAt,closingIssuesReferences \
  --jq '"\(.createdAt) \(.mergedAt // "-") \(.closingIssuesReferences[0].number // "-")"')
ready=$(gh api "repos/$repo/issues/$pr/events" --paginate --jq '[.[]|select(.event=="ready_for_review")|.created_at]|last // empty' | tail -1)
ready=${ready:-$created}
claim=-
[ "$issue" = - ] || claim=$(gh api "repos/$repo/issues/$issue/events" --paginate \
  --jq '[.[]|select(.event=="labeled" and .label.name=="in-progress")|.created_at]|last // empty' | tail -1)
claim=${claim:--}
ep() { if [ "$1" = - ]; then echo -; else date -j -u -f %Y-%m-%dT%H:%M:%SZ "$1" +%s; fi; }
mins() { if [ "$1" = - ] || [ "$2" = - ]; then echo -; else echo $(( ($2 - $1) / 60 )); fi; }
c=$(ep "$claim") d=$(ep "$created") r=$(ep "$ready") m=$(ep "$merged")
load=$(sysctl -n vm.loadavg | awk '{print $3}')
[ -s "$log" ] || printf 'merged_at\tpr\tissue\tmodel\tclaim_to_draft\tdraft_to_ready\tready_to_merged\ttotal_min\tgate\tgate_s\tbatched\tagents\tload5\n' > "$log"
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$merged" "$pr" "$issue" "$model" \
  "$(mins "$c" "$d")" "$(mins "$d" "$r")" "$(mins "$r" "$m")" "$(mins "$c" "$m")" "$kind" "$secs" "$batched" "$agents" "$load" | tee -a "$log"
