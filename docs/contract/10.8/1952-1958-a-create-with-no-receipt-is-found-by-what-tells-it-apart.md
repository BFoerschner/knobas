- **IPC command semantics, issues #1952 and #1958 (2026-10-24): a create that named nothing in its
  receipt is found by what tells it apart, a pull request by its head and base, a ticket by one
  title rule.** Ruled by the implementer on Opus (`docs/decisions/2026-10-unattended-rulings.md`,
  `## #1952, #1958`). No signature change on a command, no new type, no migration, no event, no
  new module in `commands/` or `ipc/`; the generated mirror does not change. It narrows two reads
  #1754 and #1949 left as the fallback for a receiptless create.

  1. **A pull request is found by head and base** (#1952). `start_work::queue`'s statement, now
     `PULL_REQUEST_BY_HEAD_AND_BASE`, matches `payload->'base'->>'ref'` beside
     `payload->'head'->>'ref'`, and `pull_request_on_head` and `Steps::pull_request` take the
     base. Every reader goes through it:
     - **`start_work_run` / `start_work_retry`**: the create step's look-before-write asks for an
       open pull request from the step's head into the step's base (the one the reader may have
       changed in the review step). A pull request open from that head into another base no
       longer settles the step "already open"; the create is sent, which Gitea accepts beside it.
     - The link step's fallback (no write, or a receipt that named nothing) reads by the same pair.
     - **`submit_write`'s arrival link** (`link_on_arrival`, so `link_arrivals` at the end of every
       sync) for a sent `CreatePullRequest` whose receipt named nothing reads by the op's `head`
       and `base`. #1949's item 2 ("found by the open pull request on its head") now reads "from
       its head into its base".
     - A mirrored record whose base is absent or another string matches nothing: the create step
       sends and the forge refuses a duplicate in its own words, and the link step settles
       failed. The receipt-named path (`entity_named`, #1949) is unchanged.
     - The link step's two failed details name the pair: "no pull request from `<head>` into
       `<base>` is open -- ..." and "no open pull request from `<head>` into `<base>` has
       reached the mirror yet", so a pull request open from the head into another base is not
       contradicted by the step's own words.
  2. **One title rule for a receiptless ticket create** (#1958). **`action_item_ticket`** (and
     `create_action_item_ticket`'s answer, and the protocol read's `owed_tickets` resolution, all
     `action_item_ticket_inner`) resolves a sent `CreateTicket` by
     `sources::write_queue::ticket_filed`, now crate-visible, the read `link_arrivals` already
     used: the key the receipt named, or else the newest ticket of the title in the write's
     source that the source changed after the write was queued, within `CLOCK_SLACK` (5 minutes).
     `protocol::ticket_titled`, which took the newest ticket of the title with no time bound, is
     removed. An older ticket of the same summary is no longer named or linked while the create's
     own is on its way; the create's own is linked once a sync brings it in. #1754's item naming
     `protocol::ticket_titled` for an action item (#779) now names `ticket_filed`.
