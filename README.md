# knobas

Your tickets, code, builds, docs and monitors in one desktop app.

knobas syncs Jira, Confluence, Gitea, TeamCity and Uptime Kuma to your own machine, so everything is one search away and tied to the ticket you are working on.

This is a toy project just to see how far agents can go with minimal supervision!

**[Download](https://github.com/BFoerschner/knobas/releases/latest)** · [Try it in a browser](#try-it-in-a-browser) · [Build it from source](#build-it-from-source) · [How it works](docs/how-it-works.md)

<img alt="The ticket page for PAY-231, Retry failed SEPA payouts, chosen as the job. A yellow card on the left lists its epic, people, logged time and a table of what surrounds the ticket, each row with its worst state, and a Stop and log button. On the right are the description, attachments and conversation. A timer in the top bar reads 0:34:00." src="docs/screenshots/job.webp">

<sub>Made-up data from a fictional company. Each release redraws these pictures. These are from <!-- shots-version -->v3.2.9-ux4<!-- /shots-version -->.</sub>

## Everything about a ticket on one page

Open a ticket and its code, builds, docs and monitors sit on one card, so you see what is failing before you open anything. Make it your job and the card turns yellow, with the timer at its foot.

## Search everything from one box

Press ⌘K, or Ctrl+K on Windows and Linux, to find tickets, pages, pull requests and more, then act on them from the keyboard. Save a search as a list, pin favourites to the top, and bind your own keys to the actions you use.

<img alt="The launcher open over the Tickets page with the query payout. Tickets come first, then ticket views and link sets, each row with a kind monogram and, where two sources hold a ticket, the source's name. The footer lists the keys for move, open, actions, clear the words, everything and help, and a Save as list button." src="docs/screenshots/launcher.webp">

## One inbox for whatever needs you

Review requests, mentions, failed builds and alerts land in one list, each with its next step on the row.

<img alt="The Inbox: a list of rows such as a review request, a monitor that is down and a failed build, each naming its job, with buttons like Ack, Re-run, Snooze and Done." src="docs/screenshots/inbox.webp">

## All your tickets on one board

Every ticket system together, in views you build as a board or a list. Jira's own boards and filters arrive as views too.

*Start work* shows its steps before anything happens. Run them and it creates the branch and a draft pull request, links it to the ticket, moves the ticket and starts the timer.

<img alt="The Tickets page. Saved views on the left, grouped into Mine, Team, Epics, Projects and Jira boards. A ticket board in the middle with To do, In progress, In review and Done columns. A preview of the selected ticket on the right with a Start work button." src="docs/screenshots/tickets.webp">

## Review code without leaving

Read diffs, comment, approve and merge from the same window. Edit a file, make a branch or open a pull request the same way, and open any of it in your own editor.

Where you have the repository cloned, knobas reads files and diffs from the clone, so browsing does not wait on the server. *Clone all* clones the ones you lack.

<img alt="Pull request 142, Idempotent retry window, on its Files changed tab. A tree of changed files on the left with its keyboard shortcuts, and a syntax-highlighted unified diff of src/sepa/retry.rs on the right with word-level marks and a resolved comment thread." src="docs/screenshots/pull-request.webp">

Files draw as what they are: Markdown, diagrams, PDFs and images.

<img alt="The Files tab of the payout-service repository with docs/flow.d2 selected in the file tree, drawn as a D2 diagram of boxes for shop, api, db and psp joined by labelled arrows. A Render as menu sits above it." src="docs/screenshots/file.webp">

## See why a build failed

A failed run opens on the failing test, and tells you whether it failed before. Re-run it as it was, or run it again with other parameters.

<img alt="Build 1187 of Payout_IntegrationTests, marked Failed. A coloured bar shows each build step and how long it took. A red panel names the failed test and its assertion, says it also failed in build 1185 on this branch and that build 1186 on main passed, and offers Show in log, Open the test file and Create ticket. Pipelines with their recent results fill the left rail." src="docs/screenshots/build.webp">

## Confluence pages, drawn in full

Read whole pages here, with their embeds, includes and tables. Edit a section, add a page or leave a comment. The pages that mention you come first.

<img alt="The Docs page showing a page called SEPA payout retry design: a tree of the Engineering space on the left, the page in a serif typeface in the middle with an info panel that names ticket PAY-231 and its status, a YAML code block, and an On this page outline on the right." src="docs/screenshots/docs.webp">

## Every monitor at a glance

All your Uptime Kuma checks with their last 24 hours. Acknowledge an alert, pause a check or hide one you do not care about, from its row.

<img alt="The Monitors table: six checks with their state, such as down, slow, pending and up, a 24-hour bar, response time and 30-day uptime, with Pause or Resume, Hide and Refresh on each row, and Ack on the one that is down." src="docs/screenshots/monitors.webp">

## Link sets, for what belongs together

Name the repositories, pipelines, pages, assets and monitors that belong to an epic or a project, once. Every ticket under it then shows them on its card. Nothing is copied onto the tickets and nothing is written to Jira.

<img alt="The Link sets page: five sets in a table, among them Ledger stack and Payout stack. Each row says what is in the set, such as 2 repositories, 8 pipelines, 2 pages, 2 assets and 1 monitor, which tickets it applies to, such as every ticket under epic PAY-200, and how many tickets that is now. A New link set button sits at the top right." src="docs/screenshots/link-sets.webp">

## Time tracking that fills in your worklogs

Run a timer on a ticket and log it to Jira as a worklog. If you switch it on, knobas also notes which item you had open, and offers that time for you to add.

<img alt="The Time page for one day. Totals read 4h 7m tracked, 35m logged and 2h 10m to log. A strip from 08:00 to 18:00 shows tracked, logged, offered and running blocks. Below it each block is a row with its state and buttons such as Log, Add to PAY-231 and Dismiss." src="docs/screenshots/time.webp">

## Your standup, already drafted

Yesterday, today and blockers, ready to copy or publish to Confluence.

<img alt="The Standup tab. A digest on the left lists yesterday, today, blockers and infrastructure, with a Copy as text button. A protocol on the right has a notes field per attendee, action items, and a Publish to Confluence button." src="docs/screenshots/standup.webp">

## Bring your own sources

Connect the built-in sources, or install a sandboxed adapter for anything else.

<img alt="Settings on its Sources page: seven sources in a list, each with a green dot, its host, its item count and when it last synced, with Add a source and Sync all buttons above." src="docs/screenshots/sources.webp">

## And more

- An asset tree of your servers, containers and services, with the monitors that watch them and the tickets that touch them.
- Your own commands as file renderers, and steps such as jq or gunzip before a file is drawn.
- A desktop notification for a new inbox item, per category, off until you switch it on.
- Backups on a schedule, and a share export that hands a colleague your views, link sets and source settings without a credential.
- Light and dark, and your own colour for any state word.

## It stays on your machine

Your data syncs to a database on your computer, and your credentials stay in the OS keychain. Anything you change goes through a queue you can see, and a conflict waits for you to decide. [How it works](docs/how-it-works.md) has the pictures.

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

Then open <http://localhost:1420/?fake-ipc>. Add `&fake-sources-healthy&fake-timer` to the address for the calm state the pictures show: every source healthy and a timer running.

## Build it from source

You need [rustup](https://rustup.rs), Node and [`just`](https://github.com/casey/just).

```bash
just deps
just dev
```

[docs/development.md](docs/development.md) has everything else.

## Built by agents

Agents wrote knobas, in over 1,100 merged pull requests in its first eleven weeks. Björn makes the decisions, and [the working model](docs/agents/working-model.md) has the rest. [`docs/`](docs/README.md) has a map of everything written down.
