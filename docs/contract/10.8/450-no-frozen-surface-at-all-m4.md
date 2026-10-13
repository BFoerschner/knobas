- **No frozen surface at all, issue #450 (2026-09-07):** **M4.1's exit witness** — the alert chain
  proven against the real Uptime Kuma. Recorded because the *absence* is the finding: no migration,
  no command, no DTO, no DTO field, no event, neither barrel, nothing under
  `crates/knobas-source/**` or `crates/knobas-http/**`, and not one line of `src/` in any crate.
  What landed is one `#[ignore]`d integration test
  (`crates/knobas-app/tests/alert_chain_live.rs`), two lines and a trap on the `kuma-live` recipe,
  and the documentation that says who knocks the canary over.

  **It is in `knobas-app` and not in `knobas-source-kuma` for the reason #389 gives**: the inbox is
  an app-crate read, so the adapter's own suite cannot reach `inbox_items_inner` at all. It runs
  *after* the adapter suite in the same recipe, creates no monitor, and so disturbs neither that
  suite's exact-set assertions nor its litter clearing — `teamcity-live-seeded`'s shape, ticket for
  ticket.

  **What it certifies that no seam could.** Every M4.1 suite before it writes what the layer under
  it was supposed to produce: `knobas-sync`'s `tests/alerts.rs` inserts the samples, `knobas-core`'s
  `tests/inbox.rs` inserts the alert row, `tests/inbox_ipc.rs` inserts both. This one inserts none
  of them — a real poll of a real Kuma makes the sample, the engine's reconcile inside that run's
  transaction opens the alert, the shipped ack clears the item, and the next poll closes it. It
  also re-asserts #445's import rule against the **server** rather than against
  `support/metrics.txt`, which is a recording of that server and therefore agrees with it by
  construction: the estate file's seven monitor names, resolved live (`(23, 9, 7, 7)`).

  **The canary's asset is made by the test and is not in `testenv/hetzner/estate.json`.** That
  file's README records of the seeded list that the canary *"watches nothing and so appears in no
  asset"* (#441), and this ticket does not overturn it: the file is the estate the products run on,
  the canary is a responder that exists to be knocked over, and a `service` under the notebook's
  `hypervisor` is not a pair the type table suggests either. So the asset and its `monitored-by`
  link are drawn through the two shipped paths a reader has — `assets::create` and *Link to…* —
  and the file stays the witness rather than the subject. **Flagged for Björn at the milestone
  gate**: if the exit sentence's *"the canary's asset"* was meant to be an entry in the estate
  file, that is an estate-file edit plus a type-table one, and it is his call and not this
  ticket's.

