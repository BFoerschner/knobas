# Passwords: a role, a secret that never rests, a sync by hand (2026-11-22)

**The ask.** Björn, on PR #2479: a Passwords page for the team's password manager, Pleasant Password Server first,
built from the clickdummy in `mockups/passwords-2/`. **His hard rule: knobas must not fill the manager's audit log with
his name.**

**Taken** (ADR-0056 holds the decisions; this is why):
- **A role, not a page for one product.** A password manager is a domain like CI or a wiki. The role fixes what an
  entry's record holds, so the page reads every manager the same way, and each manager declares what else it offers.
  Bitwarden or KeePassXC later is an adapter, not a new page.
- **A secret never rests.** The mirror is a plain PostgreSQL directory on disk. A password read live and dropped
  after 30 seconds leaves nothing a stolen laptop or a backup could give away. That is why a write to the manager is
  direct and never queued: a queued write would rest.
- **By hand.** A sync of a team server may write a line under the person's name. So it runs when they press *Sync
  now*, and the page says when it last did. Björn's answer 3: sync the team servers even though a sync writes lines,
  keep *by hand* as the default, and make as few line-writing calls as possible. On Pleasant that is one sign-in per
  12 hours.
- **Codes on the machine.** A one-time code made from a key in the system's password store reads nothing from the
  manager, so it writes no line, asks no usage comment and works offline.
- **Notes and fields mirrored** (Björn's answers 1 and 2): searched in Everything search and the launcher, kept out
  of backups and share exports. The real server marks no field protected, so Fable ruled for Björn that every value it
  sends openly is mirrored, and Settings says so; a value that must not rest belongs in the entry's password.

**Measured, not assumed.** Every Pleasant behaviour the adapter leans on was confirmed on a real 9.5.4 by the server
agent and recorded (`testenv/pleasant/recorded/`): which calls write audit lines, Pleasant's own TOTP, the comment rule
on entries, the error bodies, the 20-entry cap. The adapter's tests replay those answers.

**Not built, and why.** History and access requests: Pleasant's REST API has neither. Attachments are built because
listings name them and a read writes no line. No work server, no certificates, no second factor (Björn).

**Reversing it.** A manager that marks protected fields keeps them out of its records with no role change. Syncing on
a schedule is one setting per source. A manager whose sync writes no line can declare `by_hand_by_default: false`.
