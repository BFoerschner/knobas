# knobas

This is a toy project just to see how far agents can go with minimal supervision!

knobas is a personal work cockpit. It is a desktop app that syncs Jira, Confluence, Gitea, TeamCity and Uptime Kuma into one Postgres on your own machine, searches all of them from one box, and ties what it finds to the ticket you are working on.

[Download the latest release](https://github.com/BFoerschner/knobas/releases/latest) for macOS, Windows or Linux, [try the interface in a browser](#try-it-in-a-browser), or [build it from source](#build-it-from-source).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/job-dark.webp">
  <img alt="The ticket page for PAY-231, Retry failed SEPA payouts, chosen as the job. A yellow card on the left lists its epic, people, logged time and a table of what surrounds the ticket, each row with its worst state, and a Stop and log button. On the right are the description, attachments and conversation. A timer in the top bar reads 0:34:00." src="docs/screenshots/job-light.webp">
</picture>

<sub>Every picture on this page is the app's real interface on made-up data, from a fictional company called Tidewater. Each release redraws them. These are from <!-- shots-version -->main<!-- /shots-version -->.</sub>

## The ticket you are on is the home page

Choose a ticket and it becomes your job, and its card turns yellow. The card counts what surrounds the ticket, sub-tasks, code, builds, docs and monitors, and each row shows its worst state, so a failing build is visible before you open anything. The description, attachments and whole conversation sit beside it. The timer runs from the foot of the card, and from the top bar on every other page.

A link set fills in what a ticket does not name itself. Name the repositories, pipelines, pages and monitors once, and they show on every ticket under an epic or in a project, with nothing copied onto the tickets or written to Jira.

## One box searches everything

⌘K opens the launcher. It searches the local copy of every source, so the answer does not wait on a server, and it finds every kind you have, down to your saved views and link sets. `#t` narrows to tickets, and every other kind has a letter too. Tab opens a row's actions, like linking it to your job. ⌘S keeps a query as a list you can come back to.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/launcher-dark.webp">
  <img alt="The launcher open over the Tickets page with the query payout. Results are grouped into ticket views, link sets and tickets, each row with a coloured kind monogram. The footer lists the keys for move, open, actions, back one step and everything, and a Save as list button." src="docs/screenshots/launcher-light.webp">
</picture>

## One inbox for everything that asks for you

Review requests, mentions, failed builds, assignments, monitors that went down and rejected sign-ins land in one stream, newest first. Each row says which job it belongs to and carries its own next step, whether that is Review, Re-run or Ack, beside Snooze and Done.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/inbox-dark.webp">
  <img alt="The Inbox: a list of rows such as a review request, a monitor that is down and a failed build, each naming its job, with buttons like Review, Open monitor, Ack, Open log, Re-run, Snooze and Done." src="docs/screenshots/inbox-light.webp">
</picture>

## Every ticket system on one board

Tickets reads every ticket system you connect. Saved views run down the left: your work, your team's, each epic and project, and your Jira boards as Jira defines them. *Start work* on a ticket creates the branch and pull request, moves it to In Progress and starts the timer, and you review each step.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/tickets-dark.webp">
  <img alt="The Tickets page. Saved views on the left, grouped into Mine, Team, Epics, Projects and Jira boards. A ticket board in the middle with To Do, In Progress, In Review and Done columns. A preview of the selected ticket on the right with a Start work button." src="docs/screenshots/tickets-light.webp">
</picture>

## Review code in the same window

Repositories, branches, commits and pull requests come from Gitea or Forgejo. Every diff uses one view: split or unified, word-level marks, expandable context, a file tree and a *since my last review* range. Draft comments survive a restart, and the keyboard covers it, with `j` and `k` for files, `n` and `p` for hunks and `c` for a comment.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/pull-request-dark.webp">
  <img alt="Pull request 142, Idempotent retry window, on its Files changed tab. A tree of changed files on the left with its keyboard shortcuts, and a syntax-highlighted unified diff of src/sepa/retry.rs on the right with word-level marks and a resolved comment thread." src="docs/screenshots/pull-request-light.webp">
</picture>

Files draw as what they are. Markdown, Mermaid, D2, Graphviz, PDF and images each have a renderer, code gets syntax highlighting, and you can add your own command as a renderer.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/file-dark.webp">
  <img alt="The Files tab of the payout-service repository with docs/flow.d2 selected in the file tree, drawn as a D2 diagram of boxes for shop, api, db and psp joined by labelled arrows. A Render as menu sits above it." src="docs/screenshots/file-light.webp">
</picture>

## Builds that say what broke

A TeamCity run opens on the failure: the failed test, its assertion, whether it already failed on this branch and whether `main` passed. The bar on top shows where the time went. Re-run it, run it again with changes, or jump to the line in the log or the test file from the same page.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/build-dark.webp">
  <img alt="Build 1187 of Payout_IntegrationTests, marked FAILURE. A coloured bar shows each build step and how long it took. A red panel names the failed test and its assertion, says it also failed in build 1185 on this branch and that build 1186 on main passed, and offers Show it in the log, Open the test file and Create ticket. Pipelines with their recent results fill the left rail." src="docs/screenshots/build-light.webp">
</picture>

## Confluence pages, drawn whole

Docs draws Confluence pages in the app, with their embeds, includes, task lists, expands and tables, beside a tree of the space. Pages that mention you come first, a page that tracks a ticket shows that ticket's status in line, and you can write pages from here too.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/docs-dark.webp">
  <img alt="The Docs page showing a page called SEPA payout retry design: a tree of the Engineering space on the left, the page in a serif typeface in the middle with an info panel that names ticket PAY-231 and its status, a YAML code block, and an On this page outline on the right." src="docs/screenshots/docs-light.webp">
</picture>

## Every check and its last 24 hours

Uptime Kuma's checks sit in one table, each with a 24-hour bar in half-hour segments. knobas samples them itself, because Kuma prunes raw heartbeats after about a day. Acknowledge an alert or pause a check from its row.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/monitors-dark.webp">
  <img alt="The Monitors table: six checks with their state, such as down, slow, pending and up, a 24-hour bar, response time and 30-day uptime, with Ack, Pause and Hide on each row." src="docs/screenshots/monitors-light.webp">
</picture>

## Time that becomes worklogs

Start the timer on a ticket and your day fills in on a strip. knobas also notes what was in front of you and offers those stretches as blocks, which you add to a ticket or dismiss. The day page says what is tracked, what is logged and what is still owed to Jira, and *Log all* sends the rest.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/time-dark.webp">
  <img alt="The Time page for one day. Totals read 4h 7m tracked, 35m logged and 2h 10m to log. A strip from 08:00 to 18:00 shows tracked, logged, offered and running blocks. Below it each block is a row with its state and buttons such as Log, Add to PAY-231 and Dismiss." src="docs/screenshots/time-light.webp">
</picture>

## A standup that is already drafted

The digest comes from what knobas saw since your last working day: what you did yesterday, what runs today, blockers and infrastructure trouble. Copy it as text, or take the team's notes beside it and publish the protocol to Confluence.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/standup-dark.webp">
  <img alt="The Standup tab. A digest on the left lists yesterday, today, blockers and infrastructure, with a Copy as text button. A protocol on the right has a notes field per attendee, action items, and a Publish to Confluence button." src="docs/screenshots/standup-light.webp">
</picture>

## Bring your own source

Jira and Confluence Data Center, Gitea and Forgejo, TeamCity and Uptime Kuma have built-in adapters. Anything else can be an external adapter, a WebAssembly component installed from a GitHub release. It runs under wasmtime with a deadline and memory cap on every call, gets no disk, never sees the credential and can only reach the hosts its manifest declares. knobas shows you those hosts at install and never updates an adapter on its own. [`knobas-adapter-sdk`](crates/knobas-adapter-sdk) is the Rust SDK, and [ADR-0028](docs/adr/0028-an-external-adapter-is-a-sandboxed-wasm-component-installed-from-a-release.md) has the reasoning.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/sources-dark.webp">
  <img alt="Settings on its Sources page: seven sources in a list, each with a green dot, its host, its item count and when it last synced, with Add a source and Sync all buttons above." src="docs/screenshots/sources-light.webp">
</picture>

## How it works

Adapters sync each source into one embedded PostgreSQL, and search and the app read that copy. Read the map left to right for sync and right to left for write-back. Grey arrows carry data in. The amber path is the only route that changes anything at a source.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/diagrams/system-map-dark.svg">
  <img alt="knobas system map: sources feed adapters, the sync scheduler upserts into the sync schema of one embedded Postgres, search and the Tauri app read it, and the only outbound path is the write queue flushed through the adapter's write call." src="docs/diagrams/system-map-light.svg">
</picture>

Everything you do from knobas takes that path: comments, status changes, worklogs, merges, re-runs and page edits. A write is stored and returns at once, and the flush loop is the only code that calls `Source::write`. Before sending, it compares the mirror's copy of the target with the snapshot taken when the write was queued. A held or refused write waits for a person. Nothing times out or applies itself.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/diagrams/write-queue-dark.svg">
  <img alt="Write queue states: a submitted write is pending; it becomes sent if the target is unchanged, held if the target changed in the mirror, or refused if the source rejects it; held and refused writes return to pending only when a person acts." src="docs/diagrams/write-queue-light.svg">
</picture>

Credentials stay in the OS keychain, and none reaches Postgres or crosses into the webview.

## Get it

Each [release](https://github.com/BFoerschner/knobas/releases/latest) has a macOS `.dmg` for Apple Silicon, Windows `.exe` and `.msi`, and Linux `.deb`, `.rpm` and `.AppImage` for x86_64. The first run downloads PostgreSQL 18.6 once, so it needs network.

Nothing is signed, so macOS and Windows object. macOS says the app is damaged, which it is not. Clear the quarantine flag:

```bash
xattr -dr com.apple.quarantine /Applications/knobas.app
```

Windows shows SmartScreen. Choose *More info*, then *Run anyway*.

## Try it in a browser

The interface runs in a plain browser on the same made-up data as these pictures, with no Rust and no database. You need Node 20.19+ or 22.12+.

```bash
npm ci --prefix app
npm run dev --prefix app
```

Then open <http://localhost:1420/?fake-ipc>. A reload puts everything back.

## Build it from source

You need [rustup](https://rustup.rs) (`rust-toolchain.toml` pins Rust 1.99), the same Node and [`just`](https://github.com/casey/just).

```bash
just deps     # npm ci in app/
just check    # the full quality gate
just dev      # the app, default profile
just demo     # the app, demo profile, where the sample data loads
```

[docs/development.md](docs/development.md) covers the gate, profiles, releases, the crates and the frontend rules, and how `just readme-shots` makes the pictures on this page.

## Built by agents

Agents wrote knobas, in over 1,100 merged pull requests in its first eleven weeks. Björn makes the decisions in a grilling session that ends in a spec and tickets. A loop then hands each ticket to an implementer agent in its own worktree, and a merge-manager agent reviews it, runs the gate and merges. [The working model](docs/agents/working-model.md) has the rules and the numbers behind them, [`CONTEXT.md`](CONTEXT.md) the vocabulary, and [`docs/adr/`](docs/adr/) the decisions.
