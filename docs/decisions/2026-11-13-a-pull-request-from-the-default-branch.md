# 2026-11-13: A pull request can go from the default branch into another branch

**Asked (Björn, on PR #2479):** "I cannot make a pullrequest into any branch FROM master. fix that."

## What stopped it

Compare's *From* listed every branch but the default one, so `master` could not be chosen. Had it been, *Into* would have stayed on `master` too: choosing *From* kept *Into* as it stood, and *Into* starts on the default branch, so the form would have said "Into and From are the same branch". The rest of the page assumed the same thing: the branch's own view (the source's view of a branch) is measured against the default branch, so for the default branch it lists the latest commits and offers no pull request, and Compare read *Commits*, the file count and the open pull request's line from it. The Branches tab's default row offered no *New pull request* for the same reason.

The backend never assumed it. The compare section reads any two branches, the queued `CreatePullRequest` carries any head and base, and the Gitea and Forgejo adapters send them as given: measured against both, below.

## The ruling

- **From offers every branch, the default one too.** A branch can't go into itself, so choosing the default branch as *From* while *Into* is the default moves *Into* to the branch the reader came from, else the newest other branch (the list's order, as the header's *New pull request* reads it), and a line above the form says so: *Into moved to feature/x, the branch you came from.* or *Into is release/1.4, the newest other branch.* An *Into* the reader picked stays where it is. One the page moved goes back to the default branch once *From* leaves it. From the default branch the address always names *Into* (`compare/<into>...<default>`).
- **For the default branch, the page reads no branch view.** Its diff, the file count and *Commits* are the source's compare from *Into*; Gitea's compare now lists its commits in the diff's range (it already read them for the two shas). A diff predicted at the checkout lists none, so they come with the check that runs behind it (ADR-0041), and local git runs nothing new (ADR-0036 unchanged).
- **The title names both ends**: *Merge main into release/1.4*, following *Into* until the reader types over it. A single commit's subject does not replace it (F-241 is about a branch's own work).
- **A pull request sent from the default branch holds back only that *Into*.** Its echo carries the base, so the form stays for another *Into*, and the line says *A pull request from main into release/1.4 is on its way*.
- **The Branches tab's default row offers *New pull request*** where another branch's view shows the repository takes pull requests (a mirror, or one with them switched off, offers none on any branch). It opens `compare/<default>`, and Compare picks *Into*.

Kept as they were: rows stay where they first drew, the held line, the arriving `Enter` hold on *Title*, keys and focus. The header's *New pull request* still proposes a branch other than the default one: it is the common case, and *From* is one choice away.

## Tested

- Vitest: `TfRepoCompare.fromDefault.test.svelte.ts` (failing on the code before: all five of its first cases), and `TfRepoWrites.test.svelte.ts`'s two cases that held the old rule, now the new one.
- Gitea's compare lists its commits: `view_compare::a_compare_from_the_default_branch_lists_its_commits_in_its_range`.
- Live, Gitea and Forgejo on the WSL box: `live_gitea::a_pull_request_goes_from_the_default_branch_into_another`, on a scratch repository (`knobas-live-from-default`) created and removed by the test, so the seeded and scale data stay as found. The compare lists the commit `main` carries, and the adapter opens head `main` into `release/scratch`, which the server lists so.
- Press trips (`--strict`): the default row's *New pull request* and *Create pull request* from the default branch, twice by key and twice with the pointer still; the create also left for the inbox and back while its toast is up (`leave-back`'s new `backTo`, since the press itself lands on *Pull requests*).

## Reversing it

Take the default branch out of `heads` in `TfRepoCompare.svelte` and the default row's button out of `TfRepoBranches.svelte`; the rest is inert without them. The range's commits in Gitea's compare are additive and can stay.
