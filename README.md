# knobas

Your tickets, code, builds, docs and monitors in one desktop app.

knobas syncs Jira, Confluence, Gitea, TeamCity and Uptime Kuma to your own machine, so everything is one search away and tied to the ticket you are working on.

This is a toy project just to see how far agents can go with minimal supervision!

**[Download](https://github.com/BFoerschner/knobas/releases/latest)** · [Try it in a browser](#try-it-in-a-browser) · [Build it from source](#build-it-from-source) · [How it works](docs/how-it-works.md)

<img alt="The ticket page for PAY-231, Retry failed SEPA payouts, chosen as the job. A yellow card on the left lists its epic, people, logged time and a table of what surrounds the ticket, each row with its worst state, and a Stop and log button. On the right are the description, attachments and conversation. A timer in the top bar reads 0:34:00." src="docs/screenshots/job.webp">

<sub>Made-up data from a fictional company. Each release redraws these pictures. These are from <!-- shots-version -->v3.2.9-ux4<!-- /shots-version -->.</sub>

## Everything about a ticket on one page

Pick a ticket and it becomes your job. Its code, builds, docs and monitors sit on one card, so you see what is failing before you open anything.

## Search everything from one box

Press ⌘K to find tickets, pages, pull requests and more, then act on them from the keyboard.

<img alt="The launcher open over the Tickets page with the query payout. Tickets come first, then ticket views and link sets, each row with a kind monogram and, where two sources hold a ticket, the source's name. The footer lists the keys for move, open, actions, clear the words, everything and help, and a Save as list button." src="docs/screenshots/launcher.webp">

## One inbox for whatever needs you

Review requests, mentions, failed builds and alerts land in one list, each with its next step on the row.

<img alt="The Inbox: a list of rows such as a review request, a monitor that is down and a failed build, each naming its job, with buttons like Ack, Re-run, Snooze and Done." src="docs/screenshots/inbox.webp">

## All your tickets on one board

Every ticket system together, with saved views. *Start work* creates the branch and pull request and starts the timer.

<img alt="The Tickets page. Saved views on the left, grouped into Mine, Team, Epics, Projects and Jira boards. A ticket board in the middle with To Do, In Progress, In Review and Done columns. A preview of the selected ticket on the right with a Start work button." src="docs/screenshots/tickets.webp">

## Review code without leaving

Read diffs, comment and merge from the same window.

<img alt="Pull request 142, Idempotent retry window, on its Files changed tab. A tree of changed files on the left with its keyboard shortcuts, and a syntax-highlighted unified diff of src/sepa/retry.rs on the right with word-level marks and a resolved comment thread." src="docs/screenshots/pull-request.webp">

Files draw as what they are: Markdown, diagrams, PDFs and images.

<img alt="The Files tab of the payout-service repository with docs/flow.d2 selected in the file tree, drawn as a D2 diagram of boxes for shop, api, db and psp joined by labelled arrows. A Render as menu sits above it." src="docs/screenshots/file.webp">

## See why a build failed

A failed run opens on the failing test, and tells you whether it failed before.

<img alt="Build 1187 of Payout_IntegrationTests, marked Failed. A coloured bar shows each build step and how long it took. A red panel names the failed test and its assertion, says it also failed in build 1185 on this branch and that build 1186 on main passed, and offers Show in log, Open the test file and Create ticket. Pipelines with their recent results fill the left rail." src="docs/screenshots/build.webp">

## Confluence pages, drawn in full

Read and write pages here, with the ones that mention you first.

<img alt="The Docs page showing a page called SEPA payout retry design: a tree of the Engineering space on the left, the page in a serif typeface in the middle with an info panel that names ticket PAY-231 and its status, a YAML code block, and an On this page outline on the right." src="docs/screenshots/docs.webp">

## Every monitor at a glance

All your Uptime Kuma checks with their last 24 hours. Acknowledge or pause one from its row.

<img alt="The Monitors table: six checks with their state, such as down, slow, pending and up, a 24-hour bar, response time and 30-day uptime, with Pause or Resume, Hide and Refresh on each row, and Ack on the one that is down." src="docs/screenshots/monitors.webp">

## Time tracking that fills in your worklogs

Run a timer, and knobas offers up what else you worked on. Send it all to Jira when you are done.

<img alt="The Time page for one day. Totals read 4h 7m tracked, 35m logged and 2h 10m to log. A strip from 08:00 to 18:00 shows tracked, logged, offered and running blocks. Below it each block is a row with its state and buttons such as Log, Add to PAY-231 and Dismiss." src="docs/screenshots/time.webp">

## Your standup, already drafted

Yesterday, today and blockers, ready to copy or publish to Confluence.

<img alt="The Standup tab. A digest on the left lists yesterday, today, blockers and infrastructure, with a Copy as text button. A protocol on the right has a notes field per attendee, action items, and a Publish to Confluence button." src="docs/screenshots/standup.webp">

## Bring your own sources

Connect the built-in sources, or install a sandboxed adapter for anything else.

<img alt="Settings on its Sources page: seven sources in a list, each with a green dot, its host, its item count and when it last synced, with Add a source and Sync all buttons above." src="docs/screenshots/sources.webp">

## It stays on your machine

Your data syncs to a database on your computer, and your credentials stay in the OS keychain. Anything you change goes through a queue you can see, and a conflict waits for you to decide. [How it works](docs/how-it-works.md) has the details.

## Get it

[Download the latest release](https://github.com/BFoerschner/knobas/releases/latest) for macOS (Apple Silicon), Windows or Linux. The first launch downloads PostgreSQL once, so it needs network.

Nothing is signed yet, so your system will object. On macOS, run this after installing:

```bash
xattr -dr com.apple.quarantine /Applications/knobas.app
```

On Windows, choose *More info*, then *Run anyway*.

## Try it in a browser

Click around on the same made-up data as these pictures. You only need Node 20.19+ or 22.12+.

```bash
npm ci --prefix app
npm run dev --prefix app
```

Then open <http://localhost:1420/?fake-ipc>.

## Build it from source

You need [rustup](https://rustup.rs), Node and [`just`](https://github.com/casey/just).

```bash
just deps
just dev
```

[docs/development.md](docs/development.md) has everything else.

## Built by agents

Agents wrote knobas, in over 1,100 merged pull requests in its first eleven weeks. Björn makes the decisions, and [the working model](docs/agents/working-model.md) has the rest.
