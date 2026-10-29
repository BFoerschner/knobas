# Björn's rulings, 2026-10-24

Questions Björn ruled on 2026-10-24, each in a comment on its issue or in an agent's session.
They are his rulings, not an agent's, and they bind as such. The first two were recorded here by
the #1966 implementer, as its dispatch asked.

## #1966: a create held as unconfirmed counts as on its way

**Question.** A create the queue holds as `Unconfirmed` (#1125, #1627) reached its source and got
no answer, so it may already exist. The pages that tell a create "on its way" from the queue
counted every held create as not on its way, so they offered the create again, and `submit_write`
took the second one. Should an unconfirmed hold count as on its way, while a drift hold still
offers the create?

**Ruling: yes.** A create held as `Unconfirmed` counts as on its way (*Monitor queued* / *Ticket
queued*, and `submit_write` refuses a second create) until the reader sends it again or discards
it. A drift hold still offers the create. This supersedes the #1951, #1953 and #1959 rulings
(`docs/archive/2026-10-unattended-rulings.md`) for the unconfirmed case only; what they ruled for
a drift hold, a refusal and a withdrawal stands.

**Why.** If the first create landed, offering it again makes two Uptime Kuma monitors of one name
or two Jira tickets, and *Send again* on the held row later makes a third. The rulings it
supersedes reasoned that a held write waits on the reader in the queue. None of them separated an
unconfirmed hold, whose create may already exist, from a drift hold, whose create was never sent.

**Where it lands.** PR for #1966. The held side's `unconfirmed` key
(`knobas_core::write_queue::unconfirmed`) is what tells the two holds apart, read as
`QueuedWrite::may_have_landed` in Rust, `held_snapshot ? 'unconfirmed'` in SQL and
`mayHaveLanded` (`app/src/lib/shell/follow-write.ts`) in the app. The §10.8 entry is
`docs/archive/contract-10.8/1966-a-create-held-as-unconfirmed-is-on-its-way.md`.

**Reversing.** Cheap: one condition in `assets::ON_ITS_WAY`, one in `willNotGo`, and the field
`ActionItemTicket.may_have_landed`.

## #1934: restore stays snapshot-and-re-insert

**Question.** A restore is several steps that each commit on their own, and nothing records that
one is under way, so a process or server that dies between two steps leaves a half-finished
database that no start notices (#1934 lists the five windows). Keep spec #1619 ruling B3, or add a
marker row, or move to scratch-and-swap?

**Ruling: A.** Keep spec #1619 ruling B3. Restore stays snapshot-and-re-insert, with no marker row
and no scratch-and-swap.

**Why.** A crash mid-restore is rare, and the reader is watching, because they started the
restore.

**Reopen** if restore starts running unattended or often.

**Reversing.** As B3 recorded: a scratch-and-swap restore, or a marker row that bring-up checks.

## #2035: the checks run on the Mac, and `check.yml` and `testenv.yml` are disabled

**Question.** Four dispatched `check.yml` runs on the #2035 branch took 30 to 45 minutes each on
the two-core runner, and three were red on a timing budget or the job's own timeout. Keep
dispatching them for the Linux opinion, or not?

**Ruling** (Björn, in the #2035 implementer's session): "do the checks locally instead of on git,
the checks take too long there. just disable the actions and run it on the machine." The
implementer disabled `check.yml` and `testenv.yml` (`gh workflow disable`), and the PR's proof is
the local gate. `release.yml` is untouched.

**What it changes.** A ticket, brief or milestone step that says `gh workflow run check.yml` or
`testenv.yml` (#2035's and #2036's criteria, #2027's "one `gh workflow run check.yml --ref main`")
takes the local `just check` in its place until Björn enables them again.

**Reversing.** `gh workflow enable check.yml` and `gh workflow enable testenv.yml`.
