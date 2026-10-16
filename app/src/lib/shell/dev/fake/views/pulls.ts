/**
 * `?fake-ipc`: a pull request's source view (#841), as `knobas-source-gitea`'s
 * `view/pr.rs` draws it: *Overview* (its facts, each reviewer's standing
 * verdict, the head commit's *Checks* and the description), *Files* as a
 * deferred stub with its count, *Commits*, *Conversation* (each entry a facts
 * block and its words) and the toolbar's actions in the #701 order.
 *
 * PR #142 is prototype-6's: its three checks (each linking the run the corpus
 * holds, so `resolve_url` opens it), one approval of two, the job's three
 * commits and Jonas's question with Mara's answer. Every other pull request
 * in the corpus draws a plain open (or settled) one with one passing check.
 *
 * Divergences, named: the corpus titles PR #142 *Idempotent retry window*
 * where prototype-6 writes *SEPA retry with exponential backoff*; a check's
 * state is Gitea's word (`failure`, `pending`), not prototype-6's (`failed`,
 * `running`); and the mock adapter draws no view for a pull request at all.
 *
 * Answers no command of its own: `entity_view` is `entity.ts`'s, which asks
 * {@link pullView} after the repository page's views.
 */
import type { Fact, SourceView, ViewAction, ViewBlock, ViewRow, ViewSection } from "../../../../ipc/entity";
import { CORPUS } from "../corpus";
import { JOB_ROWS } from "../job-reach";
import { PAYOUT_COMMITS, payoutCommitId } from "./repos";

const fact = (label: string, value: string, tone: Fact["tone"] = null): Fact => ({ label, value, tone });
const section = (id: string, title: string, blocks: ViewBlock[], count: number | null = null, deferred = false): ViewSection => ({
  id,
  title,
  count,
  deferred,
  blocks,
});

const action = (write_op: string, label: string, over: Partial<ViewAction> = {}): ViewAction =>
  ({
    write_op,
    label,
    fields: [],
    confirm: null,
    primary: false,
    danger: false,
    disabled: null,
    with_settings: false,
    values: {},
    entity: null,
    line_comments: "none",
    ...over,
  }) as ViewAction;

const text = (name: string, label: string, value: string | null, required = false) => ({
  name,
  label,
  kind: { type: "text" as const, suggestions: [] },
  value,
  required,
  placeholder: null,
});

/** The run a check links to: its corpus row's own URL, which `resolve_url` reads back. */
function runUrl(entityId: string): string | null {
  return JOB_ROWS.find((row) => row.entity_id === entityId)?.web_url ?? null;
}

const check = (title: string, state: string, tone: ViewRow["tone"], meta: string | null, run: string | null): ViewRow => ({
  entity_id: null,
  title,
  meta,
  tone,
  chip: state,
  url: run === null ? null : runUrl(run),
});

/** The toolbar, in the #701 order: what a reviewer says, the maintenance, then *Merge…*. */
function actions(title: string, body: string, own: boolean, open: boolean, behind: boolean, blocked: string | null): ViewAction[] {
  const list: ViewAction[] = [];
  if (open) list.push(action("submit_review", "Submit review…", { primary: true, line_comments: "requires", values: { verdict: "comment" } }));
  list.push(action("comment", "Comment…", { fields: [{ ...text("body", "Comment", null, true), kind: { type: "long_text" } }] }));
  if (open && !own) {
    list.push(action("submit_review", "Approve…", { values: { verdict: "approve" } }));
    list.push(action("submit_review", "Request changes…", { values: { verdict: "request_changes" } }));
  }
  list.push(
    action("edit_pull_request", "Edit…", {
      fields: [text("title", "Title", title, true), { ...text("body", "Description", body), kind: { type: "long_text" } }],
    }),
  );
  if (open) {
    list.push(
      action("edit_pull_request", "Close", {
        values: { state: "closed" },
        danger: true,
        confirm: "Close this pull request without merging it?",
      }),
    );
    if (behind) {
      list.push(
        action("update_pull_request_branch", "Update branch…", {
          confirm: "Bring this pull request up to date with main?",
          fields: [
            {
              name: "style",
              label: "How",
              kind: { type: "choice", options: [{ value: "merge", label: "Merge main in" }, { value: "rebase", label: "Rebase onto main" }] },
              value: null,
              required: true,
              placeholder: null,
            },
          ],
        }),
      );
    }
    list.push(action("merge_pull_request", "Merge…", { primary: true, disabled: blocked }));
  }
  return list;
}

const PR_142 = "mock:payout-service#142";
const DESCRIPTION_142 =
  "Retries SEPA payouts on HTTP 503 and TEMP_UNAVAILABLE with exponential backoff: base 30 s, factor 2, at most 5 attempts, jitter ±10 %. Permanent failures go to the manual review queue. Closes PAY-231.";

function view142(): SourceView {
  const title = CORPUS.find((row) => row.entity_id === PR_142)?.title ?? "";
  const blocked = "1 of 2 required approvals; required check Payout_IntegrationTests";
  const commits = PAYOUT_COMMITS.map(
    ([sha, subject, author, at]): ViewRow => ({
      entity_id: payoutCommitId(sha),
      title: subject,
      meta: `${author} · ${at.slice(0, 10)}`,
      tone: null,
      chip: sha.slice(0, 10),
      url: null,
    }),
  );
  return {
    sections: [
      section("overview", "Overview", [
        {
          type: "facts",
          facts: [
            fact("State", "open", "info"),
            fact("Author", "mara"),
            fact("Branches", "feat/PAY-231-idempotent-retry → main"),
            fact("Mergeable", `no: ${blocked}`, "warn"),
            fact("Created", "2026-08-21 15:30"),
            fact("Updated", "2026-08-22 10:20"),
            fact("Review", "approved by priya", "ok"),
            fact("Review", "commented by jonas"),
          ],
        },
        {
          type: "entities",
          title: "Checks",
          rows: [
            check("Payout_IntegrationTests #1187", "failure", "err", "41 passed, 1 failed", "mock:Payout_IntegrationTests#1187"),
            check("Payout_Build #1188", "pending", "info", "step 3 of 5", "mock:Payout_Build#1188"),
            check("Payout_Lint #904", "success", "ok", null, "mock:Payout_Lint#904"),
          ],
        },
        { type: "text", text: DESCRIPTION_142, markdown: true },
      ]),
      section("files", "Files", [], 3, true),
      section("commits", "Commits", [{ type: "entities", title: null, rows: commits }], commits.length),
      section(
        "conversation",
        "Conversation",
        [
          { type: "facts", facts: [fact("jonas", "2026-08-22 09:30"), fact("Review", "commented")] },
          { type: "text", text: "Should the jitter be bounded? ±20 % feels wide.", markdown: true },
          { type: "facts", facts: [fact("mara", "2026-08-22 10:20")] },
          { type: "text", text: "Bounded to ±10 % in c90d11.", markdown: true },
        ],
        2,
      ),
    ],
    actions: actions(title, DESCRIPTION_142, true, true, true, `Gitea refuses this merge: ${blocked}`),
  };
}

/** Any other pull request in the corpus: open or settled, one passing check, its title as its description. */
function plainView(entityId: string): SourceView | undefined {
  const row = CORPUS.find((candidate) => candidate.entity_id === entityId && candidate.kind === "pr");
  if (row === undefined) return undefined;
  const payload = row.payload as { state?: string; merged?: boolean };
  const merged = payload.merged === true;
  const open = !merged && payload.state === "open";
  const state = merged ? fact("State", "merged", "ok") : open ? fact("State", "open", "info") : fact("State", "closed", "muted");
  const author = row.author ?? "someone";
  const description = `${row.title}.`;
  return {
    sections: [
      section("overview", "Overview", [
        {
          type: "facts",
          facts: [state, fact("Author", author), fact("Branches", "feature/work → main"), ...(open ? [fact("Mergeable", "yes")] : [])],
        },
        { type: "entities", title: "Checks", rows: [check("Build", "success", "ok", null, null)] },
        { type: "text", text: description, markdown: true },
      ]),
      section("files", "Files", [], 1, true),
      section(
        "commits",
        "Commits",
        [{ type: "entities", title: null, rows: [{ entity_id: null, title: row.title, meta: author, tone: null, chip: "5d77e1a0c3", url: null }] }],
        1,
      ),
      section("conversation", "Conversation", [], 0),
    ],
    actions: actions(row.title, description, author === "mara", open, false, null),
  };
}

/** A pull request's whole view, or one of its sections; `undefined` for anything else. */
export function pullView(entityId: string, sectionId: string | null): SourceView | undefined {
  const whole = entityId === PR_142 ? view142() : plainView(entityId);
  if (whole === undefined || sectionId === null) return whole;
  return { sections: whole.sections.filter((candidate) => candidate.id === sectionId), actions: [] };
}
