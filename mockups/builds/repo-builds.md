# Builds on the repository page

Mockup: [repo-builds.html](repo-builds.html) (2026-11-13). Its yellow bar switches screens, repositories (one with 31 branches) and theme.

The question this answers is "is my branch green, everywhere this repo is built?" On GitHub Actions and Jenkins multibranch, a pipeline belongs to a repository. The Builds page also has to answer "what is red anywhere", which a repository tab cannot. So the mockup keeps both pages.

## Screens

1. **Repository › Builds.** A new tab after *Pull requests*. Its count is the number of pipelines, or in red the number failing on `main` or your branch. The tab has a branch on the left and its verdict on the right.
   - **The branch rail** uses the Builds rail's rows: a glyph for the branch's worst latest run, the name with its namespace (`feature/`, `renovate/`) dimmed, and "1 failing" on the right where it applies. It's grouped into *Default branch*, *Yours*, *Pull requests* and *Other branches*, newest first. A filter field and *Failing* and *Mine* chips make 31 branches, or 300, easy to search. It opens on your branch.
   - **The verdict** answers the first question on a branch: does anything fail here that passes on `main`? Such a failure gets the run page's problem box (name, tests, *Log*, *Run again*, *Its runs*), the one loud thing on the page. Otherwise a quiet line says so, and says whether `main` fails too.
   - **The table** shows each pipeline's latest run on this branch next to its latest run on `main`, with a verdict word: *Only on this branch*, *Fails on main too*, *Fixed on this branch*, *Running*, *Waiting for approval*. Failures come first. Pipelines that never ran on the branch are listed in one line under the table, not as rows of dashes.
   - On `main` itself the comparison column goes, and the verdict says what fails on `main`.
   - The first version, a grid of pipelines by branches, made you read across 600 px from a name to its cell, cut off the branch names, and gave every cell a time. It also didn't answer "is this mine?". It is gone, along with its *Branch…* picker.
2. **Repository › Builds › a pipeline.** Pressing a row shows that pipeline's runs without leaving the repo. A *main | Every branch* switch picks which runs, starting on *Every branch*. A link above names the branch you came from, to go back to it. *Open in Builds* goes to its full page.
3. **Repository › Branches.** The Branches tab as it is today, plus a *Builds* column.
   - Each branch gets one cell: the icon of its pipelines' worst latest run, and words ("1 of 4 failing", "1 running", "All 5 passed").
   - A *Failing* chip joins *Mine*, *No PR* and *Merged*.
   - Pressing a cell opens the Builds tab on that branch. *Updated* stays as it was.
   - This is where many branches are handled: the tab already has a filter, chips and a count. `portal-web` has 31 branches to show it.
4. **Builds, grouped by repository.** The estate view stays and gains a *By project | By repository* switch next to *This job | All*.
   - A pipeline that builds several repositories appears under each of them.
   - When a repository mixes sources, only the less common source's rows show a tag, such as *TeamCity* among four GitHub workflows.
   - Pipelines that check out no repository (nightlies, data jobs, infrastructure) sit under *No repository*. That's the reason Builds can't simply move into Code.

The amber pause glyph on `renovate/vite-6` is the "waiting for a person" state that GitHub environments and Jenkins `input` steps need. The app doesn't have it yet.

## What it needs

- **A pipeline's repositories in the mirrored row**, declared by the adapter. Today they are only in `pipeline_settings`, a live read made when a pipeline page opens, so the Code page can't list a repository's pipelines without opening every pipeline. This is a frozen surface (§10.8) and part of the adapter interface (ADR-0028).
- **Runs per pipeline and branch.** The mirror keeps each pipeline's recent runs. A pipeline's latest run on each branch needs an index over those runs, which can't come from the twelve the rail holds. The Branches column reads it for every branch of a repository at once, so it is one read per repository, not one per branch.
- **A pipeline's default branch in the mirrored row**, so a row's glyph can mean the default branch.

## Built (2026-11-13)

On `ux-polish-repo-builds`.

**What landed:**
- **The repository's Builds tab** (`TfRepoBuilds.svelte`, `repo-builds.ts`).
- **One new read, `repo_pipelines`** (`knobas_core::builds::REPO_PIPELINES`). It answers the pipelines that built the repository's commits, each with its newest run per branch.

**How the read differs from "What it needs".** A repository's runs are the runs whose commit is one of its mirrored commits: its commits by their stored parent, their shas, `item_revision`, `item_pipeline`. The mirror already holds all of that, so there is no migration, no adapter change and no new declared path. The cost:
- a run shows only once the code host has synced its commit;
- runs of commits older than the code host's window don't show;
- runs that name no branch don't show.

**Speed.**
- The statement takes 0.6 ms on analyzed tables and 1 ms on tables never analyzed, at 1,500 builds over 20 repositories. `tests/it/read_plans.rs` asserts the plan: no link table is read whole, and no item is filtered per row.
- The read is kept by the answer cache, scoped to `ci` and `code-host`, so a return draws from memory.
- The repository page holds the tab mounted.

**Changes from the mockup:**
- *Yours* is the checkout's branch plus the branches naming the job.
- The rail holds the order it first drew in (rows stay where they drew).
- *Pull requests* lists the branches an open pull request is opened from. The same `repo_pipelines` answer carries them (`pull_requests`), read from each pull request's declared `head`, a fork's left out.

**Also built (second pass, the same day):**
- **The repository page reads `repo_pipelines` once.** The tab's count (in red, the pipelines failing on the default branch or yours, else how many build the repository), the Builds tab and the Branches tab's *Builds* column share it.
- **The Branches tab's *Builds* column.** Each branch shows the worst of its pipelines' newest runs ("1 of 7 failing", "All passed", "No builds") and links to the Builds tab at that branch.
- **A pipeline's runs inside the tab** (`builds/<branch>/<pipeline>`). They come from `pipeline_runs`, filtered to *main* or *Every branch* of the repository, and *Open in Builds* opens its whole page.
- ***By project | By repository* on the Builds page.** Each run already names its commit, so one more read, `commit_repositories`, names the repositories, read only while *By repository* is on. A repository's fold holds every pipeline that built it, and *No repository* the rest. Where two sources share a fold, the fewer's rows say their source.
