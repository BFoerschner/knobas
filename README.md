# knobas

A desktop app (Rust + Tauri) that pulls Jira, Confluence, Gitea, TeamCity and similar sources into one place: one configuration per source, a local Postgres full-text index so search actually works, links between tickets / branches / PRs / builds / pages / notes, local notes, write-back (edit tickets and pages, trigger builds, git operations), time tracking with worklogs assembled from what you actually did, an attention inbox, and generated standup digests.

`mockups/` holds the design rounds: clickable HTML mockups with shared example data, used to choose the UI before the app is built. `mockups/shared/` is the brief every mockup is built from.
