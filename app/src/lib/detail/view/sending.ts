/**
 * What a write on its way draws in the detail (#752): the queue's own row,
 * read for the place it shows in and the words it shows.
 *
 * A write returns once it is queued, and the send happens behind the reader.
 * So until it settles the panel draws what the write *will* make, muted and
 * marked *sending…*: a comment at the end of the discussion, a new title in
 * the header, a merge as the state it will leave. It is drawn from the row
 * `pending_writes` hands back, never invented here, so a write still pending
 * after a restart is drawn the same way.
 *
 * One entry per op, as `actions.ts` is for building the payload: the shell
 * reads the payload it built, never a source's fields (ADR-0020).
 */
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";

/**
 * One thing a pending write draws:
 *
 * - `said`: words added to the discussion (a comment, a review), with the
 *   review's verdict in the reader's words;
 * - `state`: where the entity will stand, drawn as a chip in the header;
 * - `title`: the entity's new title, drawn in place of the old one;
 * - `line`: anything else, one sentence under the header.
 */
export type Echo =
  | { place: "said"; verdict: string | null; text: string }
  | { place: "state"; to: string }
  | { place: "title"; title: string }
  | { place: "line"; text: string };

const VERDICTS: Record<string, string> = {
  approve: "approved",
  request_changes: "changes requested",
  comment: "commented",
};

/** One-key payload's variant name and body. */
function variant(payload: WriteOpPayload): [string, Record<string, unknown>] | null {
  const entries = Object.entries(payload as Record<string, unknown>);
  if (entries.length !== 1) return null;
  const [name, body] = entries[0]!;
  return body !== null && typeof body === "object" ? [name, body as Record<string, unknown>] : null;
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

/** What `payload` draws while it is on its way; empty for an op with nothing to show. */
export function echoesOf(payload: WriteOpPayload): Echo[] {
  const read = variant(payload);
  if (read === null) return [];
  const [name, body] = read;
  switch (name) {
    case "Comment":
      return text(body.body) ? [{ place: "said", verdict: null, text: body.body as string }] : [];
    case "SubmitReview": {
      const comments = Array.isArray(body.comments) ? body.comments.length : 0;
      const words = text(body.body) ?? (comments > 0 ? `${comments} line ${comments === 1 ? "comment" : "comments"}` : "");
      return [{ place: "said", verdict: VERDICTS[String(body.verdict)] ?? String(body.verdict), text: words }];
    }
    case "Approve":
      return [{ place: "said", verdict: "approved", text: text(body.body) ?? "" }];
    case "MergePullRequest":
      return [{ place: "state", to: "merged" }];
    case "DeleteBranch":
      return [{ place: "state", to: "deleted" }];
    case "Transition":
      return text(body.status) ? [{ place: "state", to: body.status as string }] : [];
    case "UpdatePullRequestBranch":
      return [{ place: "line", text: `Branch update (${String(body.style)})` }];
    case "EditPullRequest": {
      const echoes: Echo[] = [];
      if (text(body.title)) echoes.push({ place: "title", title: body.title as string });
      if (typeof body.body === "string") echoes.push({ place: "line", text: "Description edited" });
      if (text(body.state)) echoes.push({ place: "state", to: body.state as string });
      return echoes;
    }
    case "RunBuild":
      return [{ place: "line", text: body.branch ? `Build on ${String(body.branch)}` : "Build on the default branch" }];
    case "TriggerBuild":
      return [{ place: "line", text: "Build" }];
    case "RerunBuild":
      return [{ place: "line", text: "Build re-run" }];
    case "CreateBranch":
      return [{ place: "line", text: `New branch ${String(body.name)}` }];
    case "CreatePullRequest":
      return [{ place: "line", text: `New pull request: ${String(body.title)}` }];
    case "UpdatePage":
      return [{ place: "line", text: "Page edit" }];
    default:
      // A setting's edit has its own mark on its row (#668); an op this
      // table does not know draws nothing rather than a guess.
      return [];
  }
}

/**
 * The mark beside an echo: the send under way, or why it is waiting -- in the
 * write-queue panel's own words (`WriteQueue.svelte`'s `waitingBecause`), so
 * one fact reads one way. A turned-off source first: the queue never sends
 * for one, and *sending…* there would be a promise nothing keeps (#204).
 */
export function sendingMark(write: Pick<QueuedWrite, "wait_reason" | "source_enabled">): string {
  if (write.source_enabled === false) return "waiting: its source is turned off";
  switch (write.wait_reason) {
    case "unreachable":
      return "waiting: the server did not answer";
    case "unauthorized":
      return "waiting: the credential was rejected";
    default:
      return "sending…";
  }
}

/** The echoes of every write in `writes` at `place`, each with its write. */
export function echoesAt<P extends Echo["place"]>(
  writes: QueuedWrite[],
  place: P,
): { write: QueuedWrite; echo: Extract<Echo, { place: P }> }[] {
  return writes.flatMap((write) =>
    echoesOf(write.payload)
      .filter((echo): echo is Extract<Echo, { place: P }> => echo.place === place)
      .map((echo) => ({ write, echo })),
  );
}
