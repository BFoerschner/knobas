# Live products on a WSL box, around the clock, 2026-11-01

**Ruling (Björn):** *"I want you to setup everything that I can run the whole live testsuite on the wsl machine. Since then the livesuite can be open 24/7. Also make it that the livesuite of jira/confluence automatically resets the api-tokens everytime it's necessary and if necessary supply it with new testdata. … Maybe have a env file represent the location of the livesuites so you can either point it to the local machine or the wsl one."*

**The fork.** 2026-10-12 (#633) put the live products in the notebook's Docker, started on demand, one JVM product at a time, because the 8 GB VM holds one. The WSL box has the memory for all of them, so they can stay up and the JVM runs stop waiting on one another's start and removal.

**Taken:** a switch, not a replacement. `testenv/live.env` (git-ignored, in the main checkout) says `KNOBAS_LIVE_TARGET=local` or `remote` with `KNOBAS_LIVE_SSH`. With no file, the 2026-10-12 rules hold unchanged.

- The remote machine runs only Docker and sshd. The Mac drives compose and the seeds through `DOCKER_HOST=ssh://…` and forwards the product ports over one ssh master. Every URL a suite uses is the same on both targets, and the remote always runs the image pins and seeds of the worktree under test. The alternative, a checkout on the box with a keeper that re-seeds by itself, was passed over: its seeds and pins would drift from the tree under test, and its `seed-state.json` would not be the worktree's.
- On a remote target no run stops a product when it ends, and the one-JVM refusal is off. The locks stay on the Mac, where the worktrees are.
- **The box has 16 GB of physical memory** (Björn, the same day: *"handle that it doesn't freeze up"*). WSL gets physical memory less 6 GB. Every container gets a hard cap and a pinned heap (`docker-compose.remote.yml`). A memory budget stops the other JVM product started longest ago when a new one would not fit. With 10 GB, any two JVM products run together and a third stops one.
- "Resets when necessary" is read as the 3-hour timebomb licence. Jira and Confluence log in with the admin password and need no token. Before either product is started or re-seeded, an instance with under an hour of licence left (by container age) is removed and set up from empty. A seed that fails against an existing instance is retried once from empty. Both apply to the local target too.
- Uptime Kuma and the canary stay on the Mac.

**Reversing it** costs deleting `live.env`: `testenv/wsl/connect.sh --local` does that and closes the tunnel. The code paths are gated on the target. The licence renewal and the retry stay, because they fix a `live-up` session that outlives its licence.

Details: `testenv/README.md`, *Live products on a remote machine (WSL)*.
