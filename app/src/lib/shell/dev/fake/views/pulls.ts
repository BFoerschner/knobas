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
 * in the corpus draws a plain open (or settled) one with one passing check;
 * fx-rates#88 is read by a token that cannot read its comments (#2065).
 *
 * #842 and #843: *Files* answers PR #142's three files when it is asked for,
 * Jonas's thread on the jitter line and an outdated one of Priya's; *Merge…*
 * declares Gitea's three fields (style, delete flag, squash message); and the
 * overview declares *Approvals* and *Conflicts* (#994), each check its time.
 *
 * Divergences, named: the corpus titles PR #142 *Idempotent retry window*
 * where prototype-6 writes *SEPA retry with exponential backoff*; a check's
 * state is Gitea's word (`failure`, `pending`), not prototype-6's (`failed`,
 * `running`); and the mock adapter draws no view for a pull request at all.
 *
 * Answers no command of its own: `entity_view` is `entity.ts`'s, which asks
 * {@link pullView} after the repository page's views.
 */
import type { DiffFile, Fact, FileThread, SourceView, ViewAction, ViewBlock, ViewRow, ViewSection } from "../../../../ipc/entity";
import { CORPUS } from "../corpus";
import { JOB_ROWS } from "../job-reach";
import { PAYOUT_COMMITS, PAYOUT_MAIN_SHA, payoutCommitId, payoutPrFiles } from "./repos";

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

/** *Merge…*'s fields as Gitea declares them (#708, #843): the style, the delete flag, a squash's message. */
function mergeFields(squashTitle: string): ViewAction["fields"] {
  return [
    {
      name: "style",
      label: "Style",
      kind: {
        type: "choice",
        options: [
          { value: "merge", label: "Merge commit" },
          { value: "rebase", label: "Rebase" },
          { value: "squash", label: "Squash" },
        ],
      },
      value: null,
      required: true,
      placeholder: null,
    },
    { name: "delete_branch", label: "Delete branch after merge", kind: { type: "flag" }, value: "true", required: false, placeholder: null },
    text("message", "Squash commit message", squashTitle),
  ];
}

/** The toolbar, in the #701 order: what a reviewer says, the maintenance, then *Merge…*. */
function actions(title: string, body: string, own: boolean, open: boolean, behind: boolean, blocked: string | null, n = 0): ViewAction[] {
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
  }
  // *Request…* in the Reviewers panel (#2303): the people the source says a review can be asked of.
  if (open)
    list.push(
      action("request_reviewers", "Request…", {
        fields: [
          {
            name: "reviewers",
            label: "Reviewer",
            kind: { type: "choice", options: ["sven", "kai", "olu"].map((who) => ({ value: who, label: who })) },
            value: null,
            required: true,
            placeholder: null,
          },
        ],
      }),
    );
  // The source's own action (#1194): a write no role's vocabulary has, its id fixed.
  list.push(
    action("source_action", "Add label…", {
      values: { id: "add_label" },
      fields: [
        {
          name: "label",
          label: "Label",
          kind: { type: "choice", options: [{ value: "7", label: "needs-qa" }, { value: "9", label: "backport" }] },
          value: null,
          required: true,
          placeholder: null,
        },
      ],
    }),
  );
  if (open) {
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
    list.push(
      action("merge_pull_request", "Merge…", {
        primary: true,
        disabled: blocked,
        confirm: "Merge this pull request?",
        fields: mergeFields(`${title} (#${n})`),
      }),
    );
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
            fact("Approvals", "1 of 2", "warn"),
            fact("Conflicts", "none with main", "ok"),
            // The side rail (#2278): main requires the integration tests and the build.
            fact("Required checks", "0 of 2 pass", "err"),
            fact("Required check", "Payout_IntegrationTests #1187", "err"),
            fact("Required check", "Payout_Build #1188", "info"),
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
            check("Payout_IntegrationTests #1187", "failure", "err", "41 passed, 1 failed · 2 h ago", "teamcity:build:Payout_IntegrationTests-1187"),
            check("Payout_Build #1188", "pending", "info", "step 3 of 5 · 6 min ago", "teamcity:build:Payout_Build-1188"),
            check("Payout_Lint #904", "success", "ok", "7 min ago", "teamcity:build:Payout_Lint-904"),
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
          {
            type: "text",
            // A mermaid fence in a comment (#2261): drawn as a diagram by the Markdown component.
            text: "Bounded to ±10 % in c90d11. The delay is now:\n\n```mermaid\nflowchart LR\n  fail[5xx] --> wait[\"2ⁿ s ± 10 %\"] --> retry[retry]\n```",
            markdown: true,
          },
        ],
        2,
      ),
    ],
    actions: actions(title, DESCRIPTION_142, true, true, true, "1 of 2 required approvals; required check Payout_IntegrationTests #1187 failed", 142),
  };
}

/**
 * PR #88 of fx-rates, read by a token with `read:repository` and `read:user`
 * only (#2065): Gitea's view opens *Conversation* on its line saying the
 * comments cannot be read (`view::muted_line`, #2042), jonas's approval
 * under it, and draws every write disabled with the scope it lacks (#2043).
 */
const READ_ONLY_PR = "mock:fx-rates#88";

const UNREAD_COMMENTS: ViewBlock = {
  type: "entities",
  title: null,
  rows: [
    {
      entity_id: null,
      title: "This token cannot read this pull request's comments: it lacks the read:issue scope",
      meta: null,
      tone: "muted",
      chip: null,
      url: null,
    },
  ],
};

/** Gitea's refusal for a write this token cannot make: the category's write scope (`grants.rs`). */
function refusedOnReadOnly(list: ViewAction[]): ViewAction[] {
  const issueWrites = new Set(["comment", "source_action"]);
  return list.map((write) => ({
    ...write,
    disabled: `This token cannot write: it lacks the ${issueWrites.has(write.write_op) ? "write:issue" : "write:repository"} scope`,
  }));
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
  const readOnly = entityId === READ_ONLY_PR;
  const drawn = actions(row.title, description, author === "mara", open, false, null, Number(entityId.split("#")[1] ?? 0));
  return {
    sections: [
      section("overview", "Overview", [
        {
          type: "facts",
          facts: [
            state,
            fact("Author", author),
            fact("Branches", "feature/work → main"),
            ...(open ? [fact("Mergeable", "yes"), fact("Approvals", "none required", "ok"), fact("Conflicts", "none with main", "ok")] : []),
          ],
        },
        { type: "entities", title: "Checks", rows: [check("Build", "success", "ok", "1 h ago", null)] },
        { type: "text", text: description, markdown: true },
      ]),
      section("files", "Files", [], 1, true),
      section(
        "commits",
        "Commits",
        [{ type: "entities", title: null, rows: [{ entity_id: null, title: row.title, meta: author, tone: null, chip: "5d77e1a0c3", url: null }] }],
        1,
      ),
      readOnly
        ? section(
            "conversation",
            "Conversation",
            [UNREAD_COMMENTS, { type: "facts", facts: [fact("jonas", "2026-08-22 09:40"), fact("Review", "approved", "ok")] }],
            1,
          )
        : section("conversation", "Conversation", [], 0),
    ],
    actions: readOnly ? refusedOnReadOnly(drawn) : drawn,
  };
}

/** The jitter line of PR #142's retry, on the new side, where Jonas asked about it. */
const JITTER_LINE = 14;

/** The commit PR #142's outdated threads were written on (#2318): `a41f2c`, the second of its three. */
const WRITTEN_ON = PAYOUT_COMMITS[1]![0];

/**
 * A thread on a file PR #142 no longer holds (#709): `src/sepa/backoff.rs`, which a41f2c added and c90d11
 * folded back into `retry.rs`. Drawn in the diff's own outdated list, at the commit it was written on (#2318).
 */
function gone142(): FileThread[] {
  return [
    {
      path: "src/sepa/backoff.rs",
      thread: {
        side: "new",
        line: 4,
        outdated: true,
        comments: [{ author: "jonas", body: "A module of its own for three lines?", at: "2026-08-22T08:30:00Z" }],
        id: "4075",
        resolved_by: null,
        replies: false,
        commit: WRITTEN_ON,
        hunk: {
          header: "@@ -0,0 +1,4 @@",
          lines: [
            { kind: "add", old: null, new: 1, text: "use std::time::Duration;" },
            { kind: "add", old: null, new: 2, text: "" },
            { kind: "add", old: null, new: 3, text: "/// The delay before attempt `n`." },
            { kind: "add", old: null, new: 4, text: "pub fn backoff(n: u32) -> Duration { Duration::from_secs(30 << n) }" },
          ],
        },
      },
    },
  ];
}

/** PR #142's *Files* once asked for (#842): its three files, Jonas's thread and an outdated one of Priya's. */
function files142(): DiffFile[] {
  const [retry, ...rest] = payoutPrFiles();
  const threads: DiffFile["threads"] = [
    {
      side: "new",
      line: JITTER_LINE,
      outdated: false,
      comments: [
        { author: "jonas", body: "Should the jitter be bounded? ±20 % feels wide.", at: "2026-08-22T09:30:00Z" },
        { author: "mara", body: "Bounded to ±10 % in c90d11.", at: "2026-08-22T10:20:00Z" },
      ],
      // A thread the source takes a reply and a resolve on (#2277).
      id: "4088",
      resolved_by: null,
      replies: true,
    },
    {
      side: "new",
      line: 3,
      outdated: false,
      comments: [{ author: "priya", body: "Name the backoff module after what it does.", at: "2026-08-21T17:50:00Z" }],
      // Resolved, so drawn collapsed (#2277).
      id: "4071",
      resolved_by: "mara",
      replies: true,
    },
    {
      side: "new",
      line: 8,
      outdated: true,
      comments: [{ author: "priya", body: "Ten attempts is a lot for a payout.", at: "2026-08-21T18:02:00Z" }],
      id: "4060",
      resolved_by: null,
      replies: false,
      // Written on a41f2c, where line 8 read `MAX_ATTEMPTS: u32 = 10` (#2318).
      commit: WRITTEN_ON,
      hunk: {
        header: "@@ -3,3 +4,5 @@",
        lines: [
          { kind: "context", old: 3, new: 4, text: "use crate::psp::errors::PspError;" },
          { kind: "context", old: 4, new: 5, text: "" },
          { kind: "del", old: 5, new: null, text: "pub const RETRY_DELAY: Duration = Duration::from_secs(60);" },
          { kind: "add", old: null, new: 6, text: "pub const BASE: Duration = Duration::from_secs(30);" },
          { kind: "add", old: null, new: 7, text: "pub const FACTOR: u32 = 2;" },
          { kind: "add", old: null, new: 8, text: "pub const MAX_ATTEMPTS: u32 = 10;" },
        ],
      },
    },
  ];
  return [{ ...retry!, threads }, ...rest];
}

/**
 * `?fake-pr-lines=<n>` (perf audit, #2200's class): PR #142 changes `n` lines,
 * twenty to a file, every fourth one added.
 */
function bulkFiles(n: number): DiffFile[] {
  return Array.from({ length: Math.ceil(n / 20) }, (_, f): DiffFile => {
    const lines = Array.from({ length: Math.min(20, n - f * 20) }, (_, i) =>
      i % 4 === 3
        ? { kind: "add" as const, old: null, new: i + 1, text: `    let added_${i} = retry(${f});` }
        : { kind: "context" as const, old: i + 1, new: i + 1, text: `    call_${i}(payout, ${f});` },
    );
    const additions = lines.filter((l) => l.kind === "add").length;
    return {
      path: `src/bulk/module_${String(f).padStart(4, "0")}.rs`,
      old_path: null,
      change: "modified",
      additions,
      deletions: 0,
      hunks: [{ header: `@@ -1,${lines.length - additions} +1,${lines.length} @@`, lines }],
      truncated: false,
      threads: [],
    };
  });
}

/** A pull request's *Files*, read on its own: PR #142's change, or one small file for any other. */
function filesSection(entityId: string, title: string): ViewSection {
  const bulk = Number(new URLSearchParams(globalThis.location?.search).get("fake-pr-lines") ?? 0) || 0;
  const files: DiffFile[] =
    entityId === PR_142 && bulk > 0
      ? bulkFiles(bulk)
      : entityId === PR_142
      ? files142()
      : [
          {
            path: "README.md",
            old_path: null,
            change: "modified",
            additions: 1,
            deletions: 0,
            hunks: [
              {
                header: "@@ -1,2 +1,3 @@",
                lines: [
                  { kind: "context", old: 1, new: 1, text: "# Notes" },
                  { kind: "context", old: 2, new: 2, text: "" },
                  { kind: "add", old: null, new: 3, text: `${title}.` },
                ],
              },
            ],
            truncated: false,
            threads: [],
          },
        ];
  const commentable = { write_op: "submit_review", reply: "reply_to_thread", resolve: "resolve_thread", head: "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192" };
  // #142's range (#2273, #2310): its commits to pick from, and the viewer last reviewed at the oldest.
  const range =
    entityId === PR_142 && bulk === 0
      ? {
          repository: "mock:tidewater/payout-service",
          base: PAYOUT_MAIN_SHA,
          head: PAYOUT_COMMITS[0]![0],
          commits: PAYOUT_COMMITS.map(([sha, subject, author]) => ({ sha, title: subject, meta: author })),
          reviewed: PAYOUT_COMMITS.at(-1)![0],
        }
      : undefined;
  return section("files", "Files", [{ type: "diff", files, truncated: false, commentable, gone: entityId === PR_142 && bulk === 0 ? gone142() : [], ...(range ? { range } : {}) }], files.length);
}

/** A pull request's whole view, or one of its sections; `undefined` for anything else. */
export function pullView(entityId: string, sectionId: string | null): SourceView | undefined {
  const whole = entityId === PR_142 ? view142() : plainView(entityId);
  if (whole === undefined || sectionId === null) return whole;
  if (sectionId === "files") {
    const title = CORPUS.find((row) => row.entity_id === entityId)?.title ?? "";
    return { sections: [filesSection(entityId, title)], actions: [] };
  }
  return { sections: whole.sections.filter((candidate) => candidate.id === sectionId), actions: [] };
}
