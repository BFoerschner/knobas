#!/bin/sh
# Phase two of the real Atlassian pair: the Tidewater Freight content of
# fixtures/tidewater/work.json in the Jira and Confluence that
# seed-atlassian.sh set up. Projects, people, issues with their comments,
# worklogs and links in Jira; the ENG space, its pages and their comments in
# Confluence.
#
#   ./seed-atlassian-content.sh            seed both products
#   ./seed-atlassian-content.sh --verify   read PAY-231, one page and the
#                                          nested page's ancestors back, walk
#                                          one throwaway ticket through the
#                                          narrowing project's workflow, and
#                                          fail if any is missing, misplaced,
#                                          or no longer narrowing (the proof
#                                          step of `just atlassian-live`)
#
# The fixture is READ-ONLY here, as it is for seed-gitea.sh: this script is
# driven from the same file crates/knobas-source-mock compiles in, so there is
# no second copy of the dataset to drift.
#
# IDEMPOTENT BY CONSTRUCTION: every create is preceded by a read, an existing
# object is a skip, and the run ends with a count of both. Re-running against a
# half-seeded instance (a run that died mid-way) finishes the seed; re-running
# against a seeded one creates nothing and exits 0. Issues are looked up by
# key, comments and worklogs by their text, pages by title in their space,
# page comments by their text.
#
# THE PROJECT TEMPLATE. Jira DC creates a project from a template key, and the
# key decides the workflow. For the fixture's own projects this script uses
#
#   com.pyxis.greenhopper.jira:basic-software-development-template
#
# Jira Software's "Basic software development" template (the three software
# templates this container lists at /rest/project-templates/1.0/templates are
# Scrum, Kanban and this one). It gives the project "Software Simplified
# Workflow for Project <KEY>", whose statuses are, read back through
# GET /rest/api/2/project/<KEY>/statuses on 2026-09-03 (Jira 10.3.24):
#
#   To Do  ->  In Progress  ->  In Review  ->  Done
#
# with every transition available from every status (transition ids 11, 21,
# 31, 41 on a fresh project). That is Jira's own default for a software
# project, not a Tidewater invention (ADR-0013); the four are exactly the
# fixture's statuses, and the live suites' transition tests depend on those
# names. Scrum and Kanban were not chosen because their workflows lack "In
# Review". The template's issue type scheme ("<KEY>: Software Development
# Issue Type Scheme") is Improvement, Task, Sub-task, New Feature, Bug and
# Epic -- no Story, which the fixture has -- so the seed adds the global
# Story type to each project's scheme through PUT /rest/api/2/issuetypescheme/
# <id>; the workflow scheme maps every type to the one workflow, so Story gets
# the same four statuses. Should a fixture status ever be missing from the
# workflow, the issue keeps the status it has and the deviation is recorded
# in seed-state.json under `jira.unreachable_statuses` -- the fixture is not
# edited to fit the server.
#
# THE SECOND PROJECT, AND WHY ITS WORKFLOW IS A DIFFERENT ONE (#522). The
# template above gives a workflow that reaches every one of its statuses from
# every one of them, so on a Jira seeded only from it the answer to "what does
# this workflow offer from here" is indistinguishable from "every status this
# project has" -- and #498's reachable-transition read, whose whole point is
# that it *narrows*, had no live witness for the narrowing. ADR-0013's rule is
# that "awkward to reproduce" is not "cannot produce", and a narrowing workflow
# is what a real Jira does all day, so this seed produces one.
#
# Measured on this container on 2026-09-08 (Jira 10.3.24), through
# /rest/project-templates/1.0/templates: the three *software* templates are
# Scrum, Kanban and Basic, all Simplified and all all-to-all, and there are
# three *business* (Jira Core) templates besides -- Project management, Task
# management and Process management. Jira Core is usable under this Jira
# Software licence (`canUserUseApplication: true`), so the business templates
# are reachable over the same POST /rest/api/2/project this file already makes.
#
#   com.atlassian.jira-core-project-templates:jira-core-process-management
#
# gives "<KEY>: Process Management Workflow", seven statuses, and this shape --
# every row a PROPER SUBSET of the seven, walked on the real server:
#
#   Open         -> In Progress                     ("Start Progress")
#   In Progress  -> Under Review, Cancelled         ("Ready For Review", "Stop Progress")
#   Under Review -> Approved, Rejected              ("Approve", "Reject")
#   Approved     -> Done                            ("Done")
#   Done         -> (nothing; terminal)
#   Cancelled    -> Open                            ("Reopen")
#   Rejected     -> In Progress                     ("Start Progress")
#
# Seven of the eight moves in that table are named for the MOVE rather than for
# the status they land on ("Start Progress" -> In Progress, "Approve" ->
# Approved, and so on; only Approved's "Done" is spelled the same as its
# `to.name`), which closes the second route by which the four-status project
# could not tell a read of `to.name` from a read of `name`. `knobas-app`'s
# atlassian_live.rs asserts the first two rows of the table; the rest of it is
# a measurement recorded here.
#
# THIS PROJECT IS DELIBERATELY EMPTY, and stays empty between runs. The Jira
# adapter's own live suite syncs the WHOLE instance (`source(json!({}))`) and
# asserts the mirror is exactly `seed-state.json`'s `jira.issues`, and its
# `Seeded::clear_leftovers` deletes every issue on the instance the seed did
# not create; a parked ticket here would have to join `jira.issues` and
# `jira.projects` to survive both, which would put a narrowing workflow under a
# restore path (`move_to`) that assumes one hop is always enough and cannot
# come back out of *Done* at all. So the witness files its own ticket, walks
# it, and deletes it. A killed run's leftover is cleared twice over: the
# witness labels its ticket `knobas-live-suite` and its own suite sweeps that
# label, and the adapter suite that runs before it deletes every issue on the
# instance the seed did not create -- which is the division this file's
# siblings already use. Nothing here is recorded in `jira.projects` or `jira.issues`
# for the same reason: those two are the FIXTURE's corpus.
#
# ISSUE KEYS MATCH THE FIXTURE. Jira allocates keys from a per-project counter
# that no REST call sets, so the fixture's PAY-231 is reached the way
# seed-gitea.sh reaches a pull request number: by burning the keys in front of
# it. Placeholders (summary "(reserved)", label `knobas-placeholder`) are
# bulk-created up to the key before the target, the real issue is created,
# and its key is ASSERTED -- a mismatch stops the run loudly, because every
# live assertion downstream is written against the fixture's keys. The
# placeholders are deleted at the end of the run, after every real issue
# exists, because while they exist a JQL `ORDER BY key DESC` reads the
# counter (deleted issues do not come back, and the counter never rewinds).
# Consequently a project this script did not create from empty cannot get the
# fixture's keys, and the script says so rather than seeding PAY-231 as PAY-7.
#
# WHO AUTHORS WHAT. Comments and worklogs are authored by the admin account,
# `knobas`: Jira DC's REST takes no author on either, and the identity-
# dependent live tests configure that username (README, "Jira and
# Confluence, end to end"). The fixture's five people exist as Jira users so
# assignees are the fixture's; they are not logged in as. Epics carry their
# summary as "Epic Name" (required on create), and a ticket's `epic` is set
# through the "Epic Link" custom field, both looked up by name from
# /rest/api/2/field. `blocked_by` becomes a "Blocks" issue link.
#
# WHAT THIS CANNOT REPRODUCE -- see README.md, "Jira and Confluence, end to
# end": created and updated timestamps (the server stamps now), the
# fixture's `spent_week_m` (a worklog sum the fixture's worklogs do not add up
# to), `assigned_by` (Jira has no such field), and a Confluence page's
# `edited` date and author. Ids the server assigns (page ids, comment ids,
# worklog ids, and this instance's Epic Link custom field id) go to
# seed-state.json so a live suite can look them up.
#
# ENDPOINT PROVENANCE. Jira: every path is in the vendored
# testenv/specs/jira-dc-rest.wadl (`project`, `user`, `field`,
# `issuetypescheme`, `issue`, `issue/bulk`, `issue/{key}/comment`, `issue/{key}/worklog`,
# `issue/{key}/transitions`, `issueLink`, `search`). Confluence has no
# machine-readable spec; `/rest/api/space`, `/rest/api/content` and
# `/rest/api/content/{id}/child/comment` are what the container named in
# seed-atlassian.sh's VERIFIED_CONFLUENCE_IMAGE answers, read off it on
# 2026-09-03. The fixture gives the ENG space no name, so it is "Engineering",
# and "Standup protocols" -- the fixture's "parent of daily protocol pages" --
# is the empty page the standup flow publishes under.
#
# THE PAGE TREE IS THIS SCRIPT'S DECISION. The fixture names no ancestors, so
# where a page sits is chosen here, and a space whose every page hangs off the
# home page cannot witness what a launcher hit's ancestor path is for: one
# segment joins nothing, so the separator and the outermost-first ordering go
# unmeasured against a real server (#388's merge report, #396). So one page is
# nested a level deeper: "SEPA payout retry design", the page M3.2's exit
# criterion names, is created under "Payments architecture overview" instead
# of under the home page, which makes its path two segments long. No page is
# invented for it -- the parent is one the fixture already has, and a design
# under the architecture overview it belongs to is the fixture's own reading.
# The nested page is created last so its parent exists whatever order
# work.json lists them in, and where each page ended up is recorded per page
# as `parent_id` in seed-state.json.
set -eu
cd "$(dirname "$0")"

FIXTURE=${FIXTURE:-../fixtures/tidewater/work.json}
STATE=seed-state.json
TEMPLATE_KEY=com.pyxis.greenhopper.jira:basic-software-development-template
# The narrowing-workflow project (#522, above): its key, its name, the Jira
# Core template whose workflow narrows, and the issue type the live witness
# files its throwaway ticket as. Named for what it is rather than after the
# Tidewater fixture, because it is not fixture content: no ticket of
# work.json belongs to it and none ever should.
NARROW_KEY=NARROW
NARROW_NAME='Narrowing workflow fixture'
NARROW_TEMPLATE=com.atlassian.jira-core-project-templates:jira-core-process-management
NARROW_TYPE=Task
PLACEHOLDER_LABEL=knobas-placeholder
PEOPLE_PASS=tidewater-dev
SPACE_KEY=ENG
SPACE_NAME=Engineering
# The one nested page and the fixture page it goes under, by fixture id (see
# "THE PAGE TREE IS THIS SCRIPT'S DECISION" above). Every other page sits
# directly under the space home.
NESTED_PAGE=sepa-design
NESTED_UNDER=payments-architecture-overview

say() { echo "seed-atlassian-content: $*"; }
die() { echo "seed-atlassian-content: $*" >&2; exit 1; }

for tool in curl jq; do
  command -v "$tool" >/dev/null || die "$tool is required"
done
[ -r "$FIXTURE" ] || die "$FIXTURE not found"
[ -r "$STATE" ] || die "$STATE not found -- run ./seed-atlassian.sh first"

# The URLs and the admin account come from what seed-atlassian.sh recorded, so
# this script and the wizard walk cannot disagree about where the products are.
JIRA_URL=$(jq -r '.jira.url // empty' "$STATE")
CONF_URL=$(jq -r '.confluence.url // empty' "$STATE")
[ -n "$JIRA_URL" ] || die "$STATE has no jira block -- run ./seed-atlassian.sh first"
[ -n "$CONF_URL" ] || die "$STATE has no confluence block -- run ./seed-atlassian.sh first"
ADMIN_USER=$(jq -r '.jira.user' "$STATE")
ADMIN="$ADMIN_USER:$(jq -r '.jira.password' "$STATE")"

# --------------------------------------------------------------------------
# One HTTP helper for both products, as in seed-gitea.sh: sets API_STATUS and
# API_BODY and never exits on an HTTP error, because "already there" is a
# normal answer for most calls and the caller decides what is acceptable.
# --------------------------------------------------------------------------
API_STATUS=0
API_BODY=''
api() {  # api <base url> <METHOD> <path> [<json body>]
  _b=$1; _m=$2; _p=$3
  if [ "$#" -gt 3 ]; then
    _raw=$(curl -sS -u "$ADMIN" -X "$_m" \
                -H 'Content-Type: application/json' -H 'Accept: application/json' \
                -w '\n%{http_code}' -d "$4" "$_b$_p")
  else
    _raw=$(curl -sS -u "$ADMIN" -X "$_m" -H 'Accept: application/json' \
                -w '\n%{http_code}' "$_b$_p")
  fi
  API_STATUS=$(printf '%s' "$_raw" | tail -n 1)
  API_BODY=$(printf '%s' "$_raw" | sed '$d')
}
jira() { api "$JIRA_URL" "$@"; }
conf() { api "$CONF_URL" "$@"; }

expect() {  # expect <what> <status>...
  _what=$1; shift
  for _s in "$@"; do [ "$API_STATUS" = "$_s" ] && return 0; done
  die "$_what: HTTP $API_STATUS: $(printf '%s' "$API_BODY" | head -c 600)"
}

jqf() { jq -r "$1" "$FIXTURE"; }
urlenc() { jq -rn --arg s "$1" '$s | @uri'; }

# The skip report. Both counters live in the main shell, so every loop below
# is a `for` over base64 rows rather than a `| while read` subshell.
CREATED=0
SKIPPED=0
created() { CREATED=$((CREATED + 1)); say "created $*"; }
skip()    { SKIPPED=$((SKIPPED + 1)); say "skip: $* already there"; }

# Merge a block into seed-state.json rather than rewriting it: seed-gitea.sh,
# seed-atlassian.sh and seed-teamcity*.sh keep their own keys in the same file.
record() {  # record <jq filter with $v bound> <json>
  jq --argjson v "$2" "$1" "$STATE" > "$STATE.tmp" && mv "$STATE.tmp" "$STATE"
}

# ==========================================================================
# --verify: the proof step. Reads PAY-231 with its worklogs, the SEPA design
# page with its body, and the nested page's ancestors back over the same REST
# APIs the seed wrote through, prints what it found, and fails if any is
# missing or somewhere else. Deliberately by fixture key and title, not by
# recorded id: this is the claim "the fixture is in there", so it must not
# need the seed's own notes to pass.
#
# ONE STEP OF IT WRITES, and it is the only one: the narrowing check below
# files a throwaway ticket in $NARROW_KEY and deletes it again, because "this
# workflow still narrows" is not a claim any read of the project can make --
# it takes a ticket standing in a state (#522). It burns a $NARROW_KEY key and
# nothing else, and that bound is the rule rather than today's arrangement:
# **--verify writes to jira.narrowing and to nothing in jira.projects, ever.**
# $NARROW_KEY's keys are asserted on by nothing; a fixture project's are
# reached by burning the keys in front of them (ISSUE KEYS MATCH THE FIXTURE
# above), so a probe filed into one would move the counter every live window.
# The delete is checked rather than assumed, and a probe left by a run killed
# between the create and the delete is cleared by the adapter suite's
# Seeded::clear_leftovers on the next run, like the live suites' own litter.
# ==========================================================================
verify() {
  _fail=0
  say "verify: GET $JIRA_URL/rest/api/2/issue/PAY-231"
  jira GET "/rest/api/2/issue/PAY-231?fields=summary,status,assignee,worklog,comment"
  if [ "$API_STATUS" != "200" ]; then
    echo "  PAY-231: HTTP $API_STATUS -- missing" >&2; _fail=1
  else
    printf '%s' "$API_BODY" | jq '{key, summary: .fields.summary, status: .fields.status.name,
      assignee: .fields.assignee.name,
      worklogs: [.fields.worklog.worklogs[] | {id, author: .author.name, started, timeSpentSeconds, comment}],
      comments: [.fields.comment.comments[] | {id, author: .author.name, body}]}'
    _want=$(jqf '.tickets[] | select(.key=="PAY-231") | .worklogs | length')
    _have=$(printf '%s' "$API_BODY" | jq '.fields.worklog.worklogs | length')
    [ "$_have" -ge "$_want" ] || { echo "  PAY-231 has $_have worklogs, the fixture $_want" >&2; _fail=1; }
    [ "$(printf '%s' "$API_BODY" | jq -r .key)" = "PAY-231" ] || { echo "  key is not PAY-231" >&2; _fail=1; }
  fi

  # The narrowing project, and that its workflow is still a narrowing one
  # (#522). Read back through the two endpoints the live witness's denominator
  # and numerator come from -- the project's statuses and one throwaway
  # issue's transitions -- because "the project exists" is not the claim: the
  # claim is that a ticket standing in the workflow's first state is offered
  # FEWER statuses than the project has, and an Atlassian template that
  # quietly became all-to-all would satisfy the first and not the second. The
  # issue is deleted again; this project holds none between runs.
  say "verify: $NARROW_KEY's workflow narrows"
  jira GET "/rest/api/2/project/$NARROW_KEY/statuses"
  if [ "$API_STATUS" != "200" ]; then
    echo "  $NARROW_KEY: HTTP $API_STATUS -- missing" >&2; _fail=1
  else
    _all=$(printf '%s' "$API_BODY" | jq -c --arg t "$NARROW_TYPE" \
             '[.[] | select(.name == $t) | .statuses[].name]')
    jira POST /rest/api/2/issue "$(jq -n --arg k "$NARROW_KEY" --arg t "$NARROW_TYPE" \
        '{fields: {project: {key: $k}, summary: "seed-atlassian-content.sh --verify: does this workflow still narrow?", issuetype: {name: $t}}}')"
    if [ "$API_STATUS" != "201" ]; then
      echo "  $NARROW_KEY: creating the probe issue: HTTP $API_STATUS: $(printf '%s' "$API_BODY" | head -c 300)" >&2; _fail=1
    else
      _probe=$(printf '%s' "$API_BODY" | jq -r .key)
      jira GET "/rest/api/2/issue/$_probe?fields=status"
      _standing=$(printf '%s' "$API_BODY" | jq -r '.fields.status.name')
      jira GET "/rest/api/2/issue/$_probe/transitions"
      _offered=$(printf '%s' "$API_BODY" | jq -c '[.transitions[].to.name] | unique')
      echo "  $_probe stands in \"$_standing\" and is offered $(printf '%s' "$_offered" | jq -r 'join(", ")') out of $(printf '%s' "$_all" | jq -r 'join(", ")')"
      jq -n --argjson o "$_offered" --argjson a "$_all" \
        -e '($o | length) > 0 and (($a - $o) | length) > 0 and (($o - $a) | length) == 0' >/dev/null \
        || { echo "  $NARROW_KEY's workflow does not narrow from \"$_standing\": offered $_offered of $_all" >&2; _fail=1; }
      jira DELETE "/rest/api/2/issue/$_probe"
      [ "$API_STATUS" = "204" ] || { echo "  deleting the probe issue $_probe: HTTP $API_STATUS" >&2; _fail=1; }
    fi
  fi

  # The page with a body, and its first `## ` heading -- from the fixture, so
  # a renamed page in work.json is still what this checks for.
  _title=$(jqf '[.pages[] | select(.body)] | first | .title')
  _heading=$(jqf '[.pages[] | select(.body)] | first | .body | split("## ")[1] | split(" — ")[0]')
  say "verify: GET $CONF_URL/rest/api/content?spaceKey=$SPACE_KEY&title=$(urlenc "$_title")"
  conf GET "/rest/api/content?spaceKey=$SPACE_KEY&type=page&title=$(urlenc "$_title")&expand=body.storage,ancestors,version"
  if [ "$API_STATUS" != "200" ] || [ "$(printf '%s' "$API_BODY" | jq '.size // 0')" -lt 1 ]; then
    echo "  page \"$_title\": HTTP $API_STATUS, $(printf '%s' "$API_BODY" | jq -r '.size // 0') results -- missing" >&2; _fail=1
  else
    printf '%s' "$API_BODY" | jq '.results[0] | {id, title, version: .version.number,
      ancestors: [.ancestors[].title], body: .body.storage.value}'
    printf '%s' "$API_BODY" | jq -e --arg h "<h2>$_heading</h2>" '.results[0].body.storage.value | contains($h)' >/dev/null \
      || { echo "  page body does not contain \"<h2>$_heading</h2>\"" >&2; _fail=1; }
  fi

  # The tree, not just the content: the nested page is what makes a launcher
  # hit's ancestor path two segments long, and a seed that put it back under
  # the home page would leave every live path assertion true and pointless.
  # By fixture title, like the read above, and against the titles the server
  # reports. A second GET of what is today the same page, because the two are
  # different claims: the read above asks for "the first page with a body",
  # this one for "the page the seed nests", and a fixture that ever gave
  # another page a body would silently stop checking the nesting if they
  # shared a request.
  _nested=$(jq -r --arg n "$NESTED_PAGE" '.pages[] | select(.id == $n) | .title' "$FIXTURE")
  _under=$(jq -r --arg u "$NESTED_UNDER" '.pages[] | select(.id == $u) | .title' "$FIXTURE")
  say "verify: ancestors of \"$_nested\""
  conf GET "/rest/api/content?spaceKey=$SPACE_KEY&type=page&title=$(urlenc "$_nested")&expand=ancestors"
  if [ "$API_STATUS" != "200" ] || [ "$(printf '%s' "$API_BODY" | jq '.size // 0')" -lt 1 ]; then
    echo "  page \"$_nested\": HTTP $API_STATUS -- missing" >&2; _fail=1
  else
    _path=$(printf '%s' "$API_BODY" | jq -r '[.results[0].ancestors[].title] | join(" > ")')
    echo "  \"$_nested\" sits at: $_path"
    printf '%s' "$API_BODY" | jq -e --arg u "$_under" \
      '[.results[0].ancestors[].title] | (length == 2) and (.[-1] == $u)' >/dev/null \
      || { echo "  \"$_nested\" is not two deep under \"$_under\" -- its ancestors are $_path" >&2; _fail=1; }
  fi
  [ "$_fail" -eq 0 ] || die "verify FAILED"
  say "verify ok: PAY-231 with its worklogs, $NARROW_KEY's narrowing workflow, \"$_title\" with its body, and \"$_nested\" under \"$_under\""
}

# The nesting is a claim about the fixture, so it is checked against the
# fixture before anything reads or writes a page: a work.json that renamed
# either id would otherwise seed a flat space and take the launcher path's
# live witness with it, quietly. Ahead of the --verify dispatch, so the proof
# step names the fixture as the cause rather than reporting a page it looked
# for under an empty title.
jq -e --arg k "$SPACE_KEY" --arg n "$NESTED_PAGE" --arg u "$NESTED_UNDER" \
  '[.pages[] | select(.space == $k) | .id] | (index($n) != null) and (index($u) != null)' \
  "$FIXTURE" >/dev/null \
  || die "the fixture's $SPACE_KEY pages must include \"$NESTED_PAGE\" and \"$NESTED_UNDER\" --
  they are the nesting the launcher's ancestor path is witnessed by."

if [ "${1:-}" = "--verify" ]; then verify; exit 0; fi
[ $# -eq 0 ] || die "unknown argument '$1'; usage: ./seed-atlassian-content.sh [--verify]"

# ==========================================================================
# Jira
# ==========================================================================
jira GET /rest/api/2/myself
expect "GET /rest/api/2/myself as $ADMIN_USER" 200

# -- people ----------------------------------------------------------------
# Five users, so an assignee can be the fixture's person. Six accounts on a
# ten-user licence. `applicationKeys` gives them Jira Software access, without
# which they are not assignable.
for row in $(jqf '.people[] | @base64'); do
  d=$(printf '%s' "$row" | base64 -d)
  username=$(printf '%s' "$d" | jq -r .username)
  jira GET "/rest/api/2/user?username=$(urlenc "$username")"
  if [ "$API_STATUS" = "200" ]; then skip "user $username"; continue; fi
  jira POST /rest/api/2/user "$(printf '%s' "$d" | jq --arg p "$PEOPLE_PASS" \
      '{name: .username, password: $p, emailAddress: "\(.id)@tidewater.test",
        displayName: .name, applicationKeys: ["jira-software"]}')"
  expect "create user $username" 201
  created "user $username"
done

# -- projects --------------------------------------------------------------
# The project key is the issue key's prefix -- one fixture ticket (PAY-236)
# carries no `project` block -- and the name is whatever ticket of that
# project names one.
PROJECTS_JSON=$(jqf '[.tickets[] | {key: (.key | split("-")[0]), name: (.project.name // empty)}]
  | group_by(.key) | map({key: .[0].key, name: (map(.name) | first)})')
PROJECT_KEYS=$(printf '%s' "$PROJECTS_JSON" | jq -r '.[].key')
for row in $(printf '%s' "$PROJECTS_JSON" | jq -r '.[] | @base64'); do
  d=$(printf '%s' "$row" | base64 -d)
  pkey=$(printf '%s' "$d" | jq -r .key)
  jira GET "/rest/api/2/project/$pkey"
  if [ "$API_STATUS" = "200" ]; then skip "project $pkey"; continue; fi
  jira POST /rest/api/2/project "$(printf '%s' "$d" | jq --arg t "$TEMPLATE_KEY" --arg l "$ADMIN_USER" \
      '{key, name, projectTypeKey: "software", projectTemplateKey: $t, lead: $l, assigneeType: "UNASSIGNED"}')"
  expect "create project $pkey" 201
  created "project $pkey ($(printf '%s' "$d" | jq -r .name), template $TEMPLATE_KEY)"
done

# -- the narrowing-workflow project ------------------------------------------
# One project, no issues, from the Jira Core process-management template (see
# "THE SECOND PROJECT" above). Find-then-skip like every create in this file,
# so a second run creates nothing; its statuses are read back off
# GET /rest/api/2/project/<KEY>/statuses -- the same endpoint the fixture
# projects' are, and a DIFFERENT one from the /transitions read the live
# witness is about, so the witness's denominator does not come from the
# endpoint under test.
jira GET "/rest/api/2/project/$NARROW_KEY"
if [ "$API_STATUS" = "200" ]; then
  skip "project $NARROW_KEY"
else
  jira POST /rest/api/2/project "$(jq -n --arg k "$NARROW_KEY" --arg n "$NARROW_NAME" \
      --arg t "$NARROW_TEMPLATE" --arg l "$ADMIN_USER" \
      '{key: $k, name: $n, projectTypeKey: "business", projectTemplateKey: $t,
        lead: $l, assigneeType: "UNASSIGNED"}')"
  expect "create project $NARROW_KEY" 201
  created "project $NARROW_KEY ($NARROW_NAME, template $NARROW_TEMPLATE)"
fi
jira GET "/rest/api/2/project/$NARROW_KEY/statuses"
expect "statuses of $NARROW_KEY" 200
# $NARROW_TYPE's own row, not `.[0]` and not a union over the types. This list
# is the DENOMINATOR the live witness calls "every status this project has",
# and the ticket that stands in it is a $NARROW_TYPE, so it has to be that
# type's workflow. `.[0]` -- which is how the fixture projects' statuses are
# read below, where one workflow serves every type -- would be some other
# type's list the day a template stopped mapping them all to one, and a longer
# list makes "proper subset" cheaper than the claim it is written to support.
printf '%s' "$API_BODY" | jq -e --arg t "$NARROW_TYPE" 'any(.[]; .name == $t)' >/dev/null \
  || die "$NARROW_KEY has no $NARROW_TYPE issue type -- the live witness files its ticket as one"
NARROW_STATUSES=$(printf '%s' "$API_BODY" | jq -c --arg t "$NARROW_TYPE" \
  '[.[] | select(.name == $t) | .statuses[].name]')
say "$NARROW_KEY offers $(printf '%s' "$NARROW_STATUSES" | jq -r 'join(", ")')"

# -- every fixture issue type in every project's scheme ----------------------
# The template's scheme has no Story. The project's scheme is found through
# its association, not by its name, and the global type is appended to it.
jira GET /rest/api/2/issuetype
expect "list issue types" 200
ALL_TYPES=$API_BODY
for pkey in $PROJECT_KEYS; do
  jira GET "/rest/api/2/project/$pkey"
  expect "read project $pkey" 200
  have_types=$(printf '%s' "$API_BODY" | jq -c '[.issueTypes[].name]')
  # A JSON array end to end: a type name may carry a space ("New Feature").
  missing=$(jq --arg p "$pkey-" --argjson have "$have_types" \
    '[.tickets[] | select(.key | startswith($p)) | .type] | unique - $have' "$FIXTURE")
  if [ "$(printf '%s' "$missing" | jq 'length')" -eq 0 ]; then
    skip "issue types of $pkey ($(printf '%s' "$have_types" | jq -r 'join(", ")'))"; continue
  fi
  missing_names=$(printf '%s' "$missing" | jq -r 'join(", ")')
  jira GET /rest/api/2/issuetypescheme
  expect "list issue type schemes" 200
  scheme=''
  for sid in $(printf '%s' "$API_BODY" | jq -r '.schemes[].id'); do
    jira GET "/rest/api/2/issuetypescheme/$sid/associations"
    expect "associations of scheme $sid" 200
    if printf '%s' "$API_BODY" | jq -e --arg k "$pkey" 'any(.[]; .key == $k)' >/dev/null; then scheme=$sid; break; fi
  done
  [ -n "$scheme" ] || die "no issue type scheme is associated with project $pkey"
  jira GET "/rest/api/2/issuetypescheme/$scheme?expand=defaultIssueType,issueTypes"
  expect "read scheme $scheme" 200
  body=$(printf '%s' "$API_BODY" | jq --argjson all "$ALL_TYPES" --argjson m "$missing" \
    '{name, description, defaultIssueTypeId: (.defaultIssueType.id // empty),
      issueTypeIds: ([.issueTypes[].id] + [$all[] | select(.name as $n | $m | index($n)) | .id])}')
  jira PUT "/rest/api/2/issuetypescheme/$scheme" "$body"
  expect "add $missing_names to scheme $scheme of $pkey" 200
  created "issue type(s) $missing_names in the scheme of $pkey"
done

# -- the two Epic custom fields, by name ------------------------------------
jira GET /rest/api/2/field
expect "list fields" 200
EPIC_NAME_FIELD=$(printf '%s' "$API_BODY" | jq -r '.[] | select(.name=="Epic Name") | .id')
EPIC_LINK_FIELD=$(printf '%s' "$API_BODY" | jq -r '.[] | select(.name=="Epic Link") | .id')
# shellcheck disable=SC2015  # `A && B || die`: dying when either test fails is the point; 0.9.0 (ubuntu-latest) flags it, 0.11.0 does not
[ -n "$EPIC_NAME_FIELD" ] && [ -n "$EPIC_LINK_FIELD" ] \
  || die "the Epic Name / Epic Link custom fields are missing -- is this a Jira Software project?"

# -- issues, ascending by key, keys asserted --------------------------------
# The highest key number the project shows. Placeholders are still present at
# this point (they go at the end), so this is the counter's value.
max_key() {  # max_key <project key> -> number, 0 for an empty project
  jira GET "/rest/api/2/search?jql=$(urlenc "project = $1 ORDER BY key DESC")&maxResults=1&fields=key"
  expect "JQL max key of $1" 200
  printf '%s' "$API_BODY" | jq -r '.issues[0].key // "X-0" | split("-")[1]'
}

# Bulk-create <n> placeholders in <project>; the last one's number is echoed.
burn() {  # burn <project key> <count>
  _left=$2; _last=0
  while [ "$_left" -gt 0 ]; do
    _n=$_left; [ "$_n" -gt 50 ] && _n=50
    jira POST /rest/api/2/issue/bulk "$(jq -n --arg p "$1" --arg l "$PLACEHOLDER_LABEL" --argjson n "$_n" \
      '{issueUpdates: [range($n) | {fields: {project: {key: $p}, issuetype: {name: "Task"},
        summary: "(reserved)", labels: [$l],
        description: "Key placeholder created by testenv/seed-atlassian-content.sh so the next issue lands on the fixture'"'"'s key. Not fixture content; deleted at the end of the seed."}}]}')"
    expect "burn $_n keys in $1" 201
    _last=$(printf '%s' "$API_BODY" | jq -r '.issues[-1].key | split("-")[1]')
    _left=$((_left - _n))
  done
  echo "$_last"
}

ISSUES=$(jqf '.tickets | sort_by(.key | split("-")[1] | tonumber) | .[] | @base64')
for row in $ISSUES; do
  d=$(printf '%s' "$row" | base64 -d)
  key=$(printf '%s' "$d" | jq -r .key)
  pkey=${key%%-*}; num=${key##*-}
  jira GET "/rest/api/2/issue/$key?fields=summary,status"
  if [ "$API_STATUS" = "200" ]; then
    # Present at its key: skipped, but only if it is the fixture's issue. A
    # placeholder or a stranger at a fixture key is a mis-key, not a skip.
    have_summary=$(printf '%s' "$API_BODY" | jq -r .fields.summary)
    want_summary=$(printf '%s' "$d" | jq -r .summary)
    [ "$have_summary" = "$want_summary" ] || die "$key exists but is \"$have_summary\", not the fixture's \"$want_summary\" --
  the keys cannot be trusted. \`docker compose --profile real-atlassian down -v jira
  jira-db confluence confluence-db\` and seed again."
    skip "issue $key"
  else
    cur=$(max_key "$pkey")
    [ "$cur" -lt "$num" ] || die "$key cannot be $key: project $pkey is already at $pkey-$cur.
  Keys come from a counter that never rewinds, so the fixture's keys are reachable
  only in a project this script created from empty. \`docker compose --profile
  real-atlassian down -v jira jira-db confluence confluence-db\` and seed again."
    if [ $((num - cur - 1)) -gt 0 ]; then
      say "burning $pkey-$((cur + 1))..$pkey-$((num - 1)) so the next issue is $key"
      last=$(burn "$pkey" $((num - cur - 1)))
      [ "$last" = "$((num - 1))" ] || die "burned up to $pkey-$last, expected $pkey-$((num - 1))"
    fi
    body=$(printf '%s' "$d" | jq --arg en "$EPIC_NAME_FIELD" --arg el "$EPIC_LINK_FIELD" --arg p "$pkey" \
      --slurpfile f "$FIXTURE" '
      . as $t
      | {project: {key: $p}, issuetype: {name: .type}, summary}
      + (if .description then {description} else {} end)
      + (if .priority then {priority: {name: .priority}} else {} end)
      + (if .assignee then {assignee: {name: ($f[0].people[] | select(.id==$t.assignee) | .username)}} else {} end)
      + (if .estimate_h then {timetracking: {originalEstimate: "\(.estimate_h)h"}} else {} end)
      + (if .type == "Epic" then {($en): .summary} else {} end)
      + (if .epic then {($el): .epic} else {} end)')
    jira POST /rest/api/2/issue "$(jq -n --argjson f "$body" '{fields: $f}')"
    expect "create issue $key" 201
    got=$(printf '%s' "$API_BODY" | jq -r .key)
    [ "$got" = "$key" ] || die "created the fixture's $key but Jira keyed it $got -- the key counter
  is not where this script believed; nothing downstream can trust the keys now.
  \`docker compose --profile real-atlassian down -v jira jira-db confluence
  confluence-db\` and seed again."
    created "issue $key ($(printf '%s' "$d" | jq -r .type): $(printf '%s' "$d" | jq -r .summary))"
    jira GET "/rest/api/2/issue/$key?fields=summary,status"
    expect "read back $key" 200
  fi

  # -- status: the fixture's, when the workflow has it ----------------------
  want=$(printf '%s' "$d" | jq -r .status)
  have=$(printf '%s' "$API_BODY" | jq -r .fields.status.name)
  if [ "$have" = "$want" ]; then
    skip "status of $key ($have)"
  else
    jira GET "/rest/api/2/issue/$key/transitions"
    expect "transitions of $key" 200
    tid=$(printf '%s' "$API_BODY" | jq -r --arg s "$want" '.transitions[] | select(.to.name==$s) | .id' | head -n 1)
    if [ -n "$tid" ]; then
      jira POST "/rest/api/2/issue/$key/transitions" "$(jq -n --arg t "$tid" '{transition: {id: $t}}')"
      expect "transition $key to $want" 204
      created "transition $key: $have -> $want"
    else
      say "$key: the workflow has no status \"$want\"; it stays \"$have\" (recorded)"
      UNREACHABLE=$(printf '%s' "${UNREACHABLE:-[]}" | jq --arg k "$key" --arg w "$want" --arg h "$have" \
        '. + [{key: $k, fixture_status: $w, seeded_status: $h}]')
    fi
  fi

  # -- comments, in fixture order, by text -----------------------------------
  jira GET "/rest/api/2/issue/$key/comment"
  expect "comments of $key" 200
  have_comments=$API_BODY
  for c in $(printf '%s' "$d" | jq -r '.comments[]? | @base64'); do
    text=$(printf '%s' "$c" | base64 -d | jq -r .text)
    if printf '%s' "$have_comments" | jq -e --arg t "$text" 'any(.comments[]; .body == $t)' >/dev/null; then
      skip "comment on $key \"$(printf '%s' "$text" | cut -c1-40)...\""; continue
    fi
    jira POST "/rest/api/2/issue/$key/comment" "$(jq -n --arg b "$text" '{body: $b}')"
    expect "comment on $key" 201
    created "comment on $key (id $(printf '%s' "$API_BODY" | jq -r .id))"
  done

  # -- worklogs, by text; `started` is the fixture's date at 09:00 UTC -------
  jira GET "/rest/api/2/issue/$key/worklog"
  expect "worklogs of $key" 200
  have_worklogs=$API_BODY
  for w in $(printf '%s' "$d" | jq -r '.worklogs[]? | @base64'); do
    wj=$(printf '%s' "$w" | base64 -d)
    text=$(printf '%s' "$wj" | jq -r .text)
    if printf '%s' "$have_worklogs" | jq -e --arg t "$text" 'any(.worklogs[]; .comment == $t)' >/dev/null; then
      skip "worklog on $key \"$text\""; continue
    fi
    jira POST "/rest/api/2/issue/$key/worklog" "$(printf '%s' "$wj" | jq \
      '{started: "\(.date)T09:00:00.000+0000", timeSpentSeconds: (.minutes * 60), comment: .text}')"
    expect "worklog on $key" 201
    created "worklog on $key (id $(printf '%s' "$API_BODY" | jq -r .id), $(printf '%s' "$wj" | jq -r .minutes) min)"
  done
done

# -- links: `blocked_by` as a "Blocks" link, once every issue exists ----------
for row in $(jqf '.tickets[] | select(.blocked_by) | {key, blocked_by: .blocked_by[]} | @base64'); do
  d=$(printf '%s' "$row" | base64 -d)
  key=$(printf '%s' "$d" | jq -r .key); by=$(printf '%s' "$d" | jq -r .blocked_by)
  jira GET "/rest/api/2/issue/$key?fields=issuelinks"
  expect "links of $key" 200
  if printf '%s' "$API_BODY" | jq -e --arg b "$by" \
       'any(.fields.issuelinks[]; .type.name=="Blocks" and .inwardIssue.key==$b)' >/dev/null; then
    skip "link $key is blocked by $by"; continue
  fi
  jira POST /rest/api/2/issueLink "$(jq -n --arg k "$key" --arg b "$by" \
      '{type: {name: "Blocks"}, inwardIssue: {key: $b}, outwardIssue: {key: $k}}')"
  expect "link $key blocked by $by" 201
  created "link $key is blocked by $by"
done

# -- the placeholders go, now that every real key is taken ------------------
SWEPT=0
while :; do
  jira GET "/rest/api/2/search?jql=$(urlenc "labels = $PLACEHOLDER_LABEL")&maxResults=100&fields=key"
  expect "JQL placeholders" 200
  keys=$(printf '%s' "$API_BODY" | jq -r '.issues[].key')
  [ -n "$keys" ] || break
  for k in $keys; do
    jira DELETE "/rest/api/2/issue/$k"
    expect "delete placeholder $k" 204
    SWEPT=$((SWEPT + 1))
  done
done
[ "$SWEPT" -eq 0 ] || say "deleted $SWEPT key placeholders"

# -- jira's ids to seed-state.json --------------------------------------------
JIRA_IDS='[]'
for row in $ISSUES; do
  key=$(printf '%s' "$row" | base64 -d | jq -r .key)
  jira GET "/rest/api/2/issue/$key?fields=comment,worklog,status"
  expect "read $key for seed-state" 200
  JIRA_IDS=$(printf '%s' "$JIRA_IDS" | jq --arg k "$key" --argjson i "$API_BODY" \
    '. + [{key: $k, id: $i.id, status: $i.fields.status.name,
           comments: [$i.fields.comment.comments[] | {id, body}],
           worklogs: [$i.fields.worklog.worklogs[] | {id, comment, timeSpentSeconds}]}]')
done
STATUSES='{}'
for pkey in $PROJECT_KEYS; do
  jira GET "/rest/api/2/project/$pkey/statuses"
  expect "statuses of $pkey" 200
  STATUSES=$(printf '%s' "$STATUSES" | jq --arg k "$pkey" --argjson s "$(printf '%s' "$API_BODY" | jq '[.[0].statuses[].name]')" '. + {($k): $s}')
done
# The argument is a jq PROGRAM: $v is jq's variable, not the shell's.
# shellcheck disable=SC2016
record '.jira += {
  template_key: $v.template, statuses: $v.statuses, author: $v.author,
  epic_link_field: $v.epic_link, projects: $v.projects, issues: $v.issues,
  unreachable_statuses: $v.unreachable, narrowing: $v.narrowing,
  _comment: "Seeded by testenv/seed-atlassian-content.sh. Jira assigns issue ids, comment ids, worklog ids and custom field ids; the keys are the fixture'"'"'s. Comments and worklogs are authored by `author`. `epic_link_field` is this instance'"'"'s Epic Link id, which is what a classic Data Center project keeps epic membership in (`fields.parent` is for sub-tasks and is absent here). `unreachable_statuses` lists fixture statuses the template'"'"'s workflow does not have. `narrowing` is the second project (#522): a Jira Core process-management project whose workflow reaches a proper subset of its own statuses from every one of them, which is what gives the reachable-transition read a live witness for narrowing. It is deliberately empty and is deliberately NOT in `projects` or `issues`, which are the fixture'"'"'s corpus."
}' "$(jq -n --arg t "$TEMPLATE_KEY" --arg a "$ADMIN_USER" --argjson ids "$JIRA_IDS" \
        --argjson un "${UNREACHABLE:-[]}" --argjson pr "$PROJECTS_JSON" --argjson st "$STATUSES" \
        --arg el "$EPIC_LINK_FIELD" \
        --argjson nw "$(jq -n --arg k "$NARROW_KEY" --arg n "$NARROW_NAME" --arg t "$NARROW_TEMPLATE" \
                          --arg it "$NARROW_TYPE" --argjson st "$NARROW_STATUSES" \
                          '{key: $k, name: $n, template_key: $t, issue_type: $it, statuses: $st}')" \
        '{template: $t, author: $a, statuses: $st, epic_link: $el, projects: $pr, issues: $ids, unreachable: $un, narrowing: $nw}')"

# ==========================================================================
# Confluence
# ==========================================================================
conf GET /rest/api/user/current
expect "GET /rest/api/user/current as $ADMIN_USER" 200

conf GET "/rest/api/space/$SPACE_KEY?expand=homepage"
if [ "$API_STATUS" = "200" ]; then
  skip "space $SPACE_KEY"
else
  conf POST /rest/api/space "$(jq -n --arg k "$SPACE_KEY" --arg n "$SPACE_NAME" '{key: $k, name: $n}')"
  expect "create space $SPACE_KEY" 200 201
  created "space $SPACE_KEY ($SPACE_NAME)"
  conf GET "/rest/api/space/$SPACE_KEY?expand=homepage"
  expect "read space $SPACE_KEY" 200
fi
HOME_ID=$(printf '%s' "$API_BODY" | jq -r '.homepage.id // empty')
[ -n "$HOME_ID" ] || die "space $SPACE_KEY has no home page"

# The fixture's page body is a flat string of "## Heading — text" sections;
# storage format wants <h2> and <p>, with `code` and *em* as their tags. A
# page with no body is created empty. A comment is one <p>. Both escape the
# same three characters, through the one jq definition below.
# shellcheck disable=SC2016
JQ_ESC='def esc: gsub("&"; "&amp;") | gsub("<"; "&lt;") | gsub(">"; "&gt;");'
comment_storage() {  # comment_storage <text>
  jq -rn --arg t "$1" "$JQ_ESC"' "<p>\($t | esc)</p>"'
}
page_storage() {  # page_storage <fixture body or empty>
  jq -rn --arg b "$1" "$JQ_ESC"'
    def inline: esc
      | gsub("`(?<c>[^`]+)`"; "<code>\(.c)</code>")
      | gsub("\\*(?<e>[^*]+)\\*"; "<em>\(.e)</em>");
    if $b == "" then "" else
      $b | split("## ") | map(select(length > 0) | rtrimstr(" "))
      | map(index(" — ") as $i
            | if $i then "<h2>\(.[:$i] | inline)</h2><p>\(.[$i+3:] | inline)</p>"
              else "<p>\(inline)</p>" end)
      | join("")
    end'
}

PAGE_IDS='[]'
# The nested page comes last: it is created under a page this same loop
# creates, so the fixture's own order cannot be relied on to put the parent
# first.
for row in $(jq -r --arg k "$SPACE_KEY" --arg n "$NESTED_PAGE" \
    '[.pages[] | select(.space == $k)]
     | map(select(.id != $n)) + map(select(.id == $n)) | .[] | @base64' "$FIXTURE"); do
  d=$(printf '%s' "$row" | base64 -d)
  title=$(printf '%s' "$d" | jq -r .title)
  fid=$(printf '%s' "$d" | jq -r .id)
  conf GET "/rest/api/content?spaceKey=$SPACE_KEY&type=page&title=$(urlenc "$title")&expand=ancestors"
  expect "find page \"$title\"" 200
  pid=$(printf '%s' "$API_BODY" | jq -r '.results[0].id // empty')
  if [ -n "$pid" ]; then
    # The parent the SERVER reports, not the one a create would have sent: a
    # page that is already there was put where it is by an earlier run, and
    # seed-state.json describes the instance rather than the intention.
    parent=$(printf '%s' "$API_BODY" | jq -r '.results[0].ancestors[-1].id // empty')
    # Named here rather than left to record an empty parent_id: a page with no
    # ancestors at all is not one this script created, and the suites that read
    # parent_id would otherwise fail three layers away on a blank id.
    [ -n "$parent" ] || die "page \"$title\" (id $pid) is already in $SPACE_KEY with no
  ancestors at all, so it is not a page this script created. \`docker compose --profile
  real-atlassian down -v jira jira-db confluence confluence-db\` and seed again."
    skip "page \"$title\" (id $pid, under $parent)"
  else
    if [ "$fid" = "$NESTED_PAGE" ]; then
      parent=$(printf '%s' "$PAGE_IDS" | jq -r --arg f "$NESTED_UNDER" \
        'map(select(.fixture_id == $f)) | .[0].id // empty')
      [ -n "$parent" ] || die "no \"$NESTED_UNDER\" page to nest \"$title\" under"
    else
      parent=$HOME_ID
    fi
    storage=$(page_storage "$(printf '%s' "$d" | jq -r '.body // ""')")
    conf POST /rest/api/content "$(jq -n --arg t "$title" --arg k "$SPACE_KEY" --arg h "$parent" --arg s "$storage" \
        '{type: "page", title: $t, space: {key: $k}, ancestors: [{id: $h}],
          body: {storage: {value: $s, representation: "storage"}}}')"
    expect "create page \"$title\"" 200
    pid=$(printf '%s' "$API_BODY" | jq -r .id)
    created "page \"$title\" (id $pid, under $parent)$(printf '%s' "$d" | jq -r 'if .note then " -- \(.note)" else "" end')"
  fi

  conf GET "/rest/api/content/$pid/child/comment?expand=body.storage&limit=100"
  expect "comments of page $pid" 200
  have_comments=$API_BODY
  comment_ids='[]'
  for c in $(printf '%s' "$d" | jq -r '.comments[]? | @base64'); do
    text=$(printf '%s' "$c" | base64 -d | jq -r .text)
    html=$(comment_storage "$text")
    cid=$(printf '%s' "$have_comments" | jq -r --arg h "$html" '.results[] | select(.body.storage.value == $h) | .id' | head -n 1)
    if [ -n "$cid" ]; then
      skip "comment on \"$title\" (id $cid)"
    else
      conf POST /rest/api/content "$(jq -n --arg p "$pid" --arg s "$html" \
          '{type: "comment", container: {id: $p, type: "page"},
            body: {storage: {value: $s, representation: "storage"}}}')"
      expect "comment on page $pid" 200
      cid=$(printf '%s' "$API_BODY" | jq -r .id)
      created "comment on \"$title\" (id $cid)"
    fi
    comment_ids=$(printf '%s' "$comment_ids" | jq --arg i "$cid" --arg t "$text" '. + [{id: $i, text: $t}]')
  done
  PAGE_IDS=$(printf '%s' "$PAGE_IDS" | jq --arg f "$fid" --arg t "$title" --arg i "$pid" --arg p "$parent" --argjson c "$comment_ids" \
    '. + [{fixture_id: $f, title: $t, id: $i, parent_id: $p, comments: $c}]')
done

# The argument is a jq PROGRAM: $v is jq's variable, not the shell's.
# shellcheck disable=SC2016
record '.confluence += {
  space: $v.space, home_page_id: $v.home, author: $v.author, pages: $v.pages,
  _comment: "Seeded by testenv/seed-atlassian-content.sh. Confluence assigns content ids; pages are found by title in the space. The fixture names no ancestors, so the seed decides the tree: every page sits under the space home page (`home_page_id`) except `sepa-design`, which is nested under `payments-architecture-overview` so that a launcher hit for it has a two-segment ancestor path. Each page records the content id it sits under as `parent_id` -- read back from the server for a page an earlier run had already created. Comments are authored by `author`."
}' "$(jq -n --arg k "$SPACE_KEY" --arg h "$HOME_ID" --arg a "$ADMIN_USER" --argjson p "$PAGE_IDS" \
        '{space: $k, home: $h, author: $a, pages: $p}')"

say "wrote $STATE"
say "done: created $CREATED, skipped $SKIPPED"
