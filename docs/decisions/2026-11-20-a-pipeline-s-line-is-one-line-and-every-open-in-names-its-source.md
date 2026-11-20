# 2026-11-20: A pipeline's line is one line, and every *Open in* names its own source (ux25)

**Found (Björn, ux24).** "The right side next to Sync now in teamcity builds it shows "Teamcity configuration, builds
<huge repo link> step 64 of the chain x > y > z > foo > bar > etc. Make it that it doesn't destroy the ui." The line under
a pipeline's title named each repository by what the source calls it, and TeamCity names a VCS root by its whole address
and branch by default (`https://git.example/org/repo.git#refs/heads/main`), in the code face, which never wraps. The
chain was a sentence of every step's name. With a 70-step chain of long names the line was 1,602 px tall at 1440 x 868,
the address ran 203 to 301 px out of its box, and at 1440 a long source name pushed the header's actions under the title.
And: "We have open in "jira, teamcity, etc." on top of the pages. What if we have several sources from different systems."

**Taken: the line.** One line at every width, the clone sign's shape (`2026-11-16`, design system, *Paths*):

- *Tidewater TeamCity build configuration, builds payout-service and 1 more. Triggered by VCS and Schedule.* The first
  repository by its own name: where the source names it by an address, the repository's last segment, no scheme, host,
  owner, `.git` or branch (`repositoryShortName`, `builds/chain.ts`). A link where the mirror holds it. *and N more* for
  the rest, and the whole name and address of each in the title of both.
- The chain is a sign (`TfChainSign`), *Step 64 of 70*, never cut. Its bubble lists the steps, numbered, this one in ink
  and scrolled to; each opens its pipeline. Closed, the chain's sentence is the sign's description. The clone's sign's
  hover, focus, pin, `Esc` and keep-in-window behaviour moved into `TfSign`, which both signs are.
- What gives way when the line is short: the triggers, then the source's words, each with an ellipsis and whole in its
  title. The repository's name only once those are gone. The owner was dropped rather than cut first: a flex row cannot
  give up the owner, then the source's words, then the name, in that order, without measuring.
- A run's *Chain on ⟨branch⟩* panel drew a box per step, ten rows for 70. Past seven steps it folds its middle: the first,
  the run's own with two either side, and the last, each gap a *… N more* a pointer presses. The keyboard's way is the
  header's *Show all 70 steps* / *Show fewer*, which keeps focus, so a second Enter folds again rather than opening a step.
  A step's name ends in an ellipsis past 22 rem, whole in its title.
- *Open in ⟨source⟩* in a header cuts a long display name at 16 rem (`.tf-btn > .tf-cut`), whole in its title. So does
  the ticket card's row.

**Taken: Open in.** Audited every *Open in ⟨source⟩*. Each opens the item's own `web_url` as its source wrote it, and
the builds, docs, ticket and item headers already named the item's source by its display name (`source_name` is
`coalesce(source_config.display_name, source_id)` in `knobas-core::builds`). Fixed where they did not:

| Surface | Was | Now |
|---|---|---|
| Monitors, an open monitor | *Open in Uptime Kuma*, whatever the source | its source's display name |
| Assets, a file's monitors | *Open in Kuma*, *Paused or gone from Kuma* | its source's display name, *Kuma* before the list is read |
| Right-click on a file, folder or line | *Open in browser*, the source known | *Open in ⟨code host's name⟩* |
| A saved view's line (Tickets) | *Open in Jira*, the ticket system, for any Jira | the view's source's display name, the system before the list is read |

Two sources of one kind (two TeamCity servers) each say their own name; an installed (wasm) adapter's items do too.
Nothing in the shell picks a name or an address by adapter kind. An adapter fills a surface by its descriptor's role
claims and payload paths, and each sync item carries its `web-url` (`knobas-wasm-host/wit/adapter.wit`). A blank display
name cannot be saved (Add source falls back to the kind's name, and no page renames one), so no fallback was added for it.

**Held by.** `testenv/ui-checks/long-paths.mjs` (`?fake-long-paths` adds a 70-step chain, `Payout_Docker` 64th, VCS roots
named by their addresses, one the mirror does not hold, and a long TeamCity display name): six Builds screens at 5120,
2560 at 125 %, 1440 x 868 and 1100 x 600. It also fails when a Builds header's line is more than one line, or its
actions leave the title's line at 1400 px and wider. Before: 3 of 24 held. After: 24 of 24, and 140 of 140 over the whole
check. Press trips `build-chain-step-key` and `build-chain-unfold-key`. Unit tests for each *Open in* surface with two
sources and an installed adapter's.

**Reversing it** is `TfPipeline`'s line back to the sentence and `chainSentence`, `TfRunChain`'s boxes unfolded, and the
four labels back to their words.
