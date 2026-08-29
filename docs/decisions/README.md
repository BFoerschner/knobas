# Decision records

Dated records of what was ruled during a stretch of work, and why. A file here names the forks
that came up, the option taken, the reasoning it rests on, and what reversing it would cost, so
that a settled question can be checked rather than re-argued. Files are named
`YYYY-MM-DD-<slug>.md`; the 2026-08-29 batch is split between a rulings file and one for the
orchestration that produced them.

## How this differs from `docs/adr/`

An ADR is a ratified architecture decision. It is numbered, it carries a `status:` header, it
fixes a shape the code is then obliged to keep, and `docs/contract.md` cites it by filename. A
record here is operational and dated: which PR merges first, how a batch was sequenced, the design
fork on a single issue, a rule about how agents work that no crate can enforce.

The two meet at one point. Where a ruling here settles something architectural, it gets an ADR of
its own and the ADR is what binds; what stays here is the reasoning and the reversal cost the ADR
compresses out. A ruling that only decides how work was run stays here.
