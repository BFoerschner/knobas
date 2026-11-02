/**
 * The ticket page's attachment previews (spec #2396, #2405): the bytes of an
 * attachment fetched live through the source's credential
 * (`attachment_bytes`), drawn as a `data:` image, and kept in memory for as
 * long as the page is open. One object per open ticket page: the page makes
 * it, hands it to the panel and the description's inline images, and drops
 * it when it closes, so nothing outlives the page and nothing reaches the
 * mirror.
 *
 * Fetching is asked for, never done on page open: the Attachments panel
 * asks for its image rows when it opens (`fetchImages`), and an inline image
 * in the description asks once it is in view (`watch`). Each file is asked at
 * most once until *Try again* (`retry`) forgets the failures. A file the list
 * says is over the 5 MiB cap is never asked.
 */
import { untrack } from "svelte";
import { SvelteMap } from "svelte/reactivity";

import { isIpcError } from "../../ipc";
import { attachmentBytes, type Attachment, type AttachmentBytes } from "../../ipc/entity";

/** The most of one attachment the page previews: 5 MiB, the backend's `ATTACHMENT_CAP`. */
export const PREVIEW_CAP = 5 * 1024 * 1024;

/**
 * Why a preview could not be fetched: `forbidden` is a 403 (this account may
 * not read the file), `refused` any other refusal of the credential (401, no
 * secret), `unreachable` everything else (the source did not answer).
 */
export type PreviewFailure = "unreachable" | "forbidden" | "refused";

export type Preview =
  | { state: "none" }
  | { state: "loading" }
  | { state: "ready"; src: string }
  | { state: "too-large" }
  | { state: "failed"; reason: PreviewFailure; message: string };

/** What the panel's stale strip says: why, and of which source. */
export type PreviewsFailure = { reason: PreviewFailure; source: string };

/** The part of an `IntersectionObserver` a watch uses. */
type Observer = { observe(node: Element): void; disconnect(): void };
type Observe = (callback: (entries: { isIntersecting: boolean }[]) => void) => Observer;

export interface Previews {
  /** The ticket whose attachments these are. */
  readonly entityId: string;
  /** The detail's attachment list. */
  readonly files: readonly Attachment[];
  /** What the stale strip says, `null` while no fetch has failed. */
  readonly failure: PreviewsFailure | null;
  /**
   * How often *Try again* was pressed (reactive). A surface that drew a
   * failure as its own placeholder, as the description's inline images do,
   * draws again when it moves.
   */
  readonly attempt: number;
  /** One file's preview (reactive). */
  of(id: string): Preview;
  /** Ask for one file, unless it is ready, under way, too large or failed. */
  fetch(id: string): void;
  /** Ask for every image in the list: what the panel does when it opens. */
  fetchImages(): void;
  /** Ask for one file and answer its `data:` image, or reject with its preview (`failed`, `too-large`). */
  load(id: string): Promise<string>;
  /** As `load`, without asking: answers once something else (a `watch`) has asked and the answer is in. */
  when(id: string): Promise<string>;
  /** Ask for one file once `node` is in view; at once where nothing can tell. Answers the stop. */
  watch(node: Element, id: string): () => void;
  /** *Try again*: forget every failure and ask again for what failed. */
  retry(): void;
  /** The page closed: forget everything, and keep no answer still on its way. */
  drop(): void;
}

/** Whether a file is drawn as a picture: its media type says image. */
export function isImage(file: Pick<Attachment, "mime">): boolean {
  return file.mime?.startsWith("image/") ?? false;
}

/** The reason a rejected read gives. */
export function failureOf(error: unknown): PreviewFailure {
  if (!isIpcError(error)) return "unreachable";
  if (error.code !== "unauthorized") return "unreachable";
  return /\bHTTP 403\b/.test(error.message) ? "forbidden" : "refused";
}

/** The browser's observer where there is one (not in jsdom). */
function browserObserve(): Observe | null {
  if (typeof IntersectionObserver === "undefined") return null;
  return (callback) => new IntersectionObserver((entries) => callback(entries));
}

const NONE: Preview = { state: "none" };

export function createAttachmentPreviews({
  entityId,
  files,
  source,
  read = attachmentBytes,
  observe = browserObserve(),
}: {
  entityId: string;
  files: readonly Attachment[];
  /** The source's configured name, for the strip. */
  source: string;
  read?: (entityId: string, attachmentId: string) => Promise<AttachmentBytes>;
  /** How a watch learns a node is in view; `null` asks at once. */
  observe?: Observe | null;
}): Previews {
  const states = new SvelteMap<string, Preview>();
  /** Bumped by `drop` and `retry`: an answer for an older round is not kept. */
  let round = 0;
  let attempt = $state(0);
  const waiting = new Map<string, ((preview: Preview) => void)[]>();

  /**
   * One file's preview, read without subscribing: `fetch` and `when` run
   * inside the description's paint, which would otherwise draw the whole
   * text again on every answer.
   */
  const peek = (id: string): Preview => untrack(() => states.get(id) ?? NONE);

  const settle = (id: string, preview: Preview) => {
    states.set(id, preview);
    if (preview.state === "loading" || preview.state === "none") return;
    for (const done of waiting.get(id) ?? []) done(preview);
    waiting.delete(id);
  };

  const failure = $derived.by((): PreviewsFailure | null => {
    for (const file of files) {
      const preview = states.get(file.id);
      if (preview?.state === "failed") return { reason: preview.reason, source };
    }
    return null;
  });

  const previews: Previews = {
    entityId,
    files,
    get failure() {
      return failure;
    },
    get attempt() {
      return attempt;
    },
    of: (id) => states.get(id) ?? NONE,
    fetch(id) {
      if (peek(id).state !== "none") return;
      const file = files.find((candidate) => candidate.id === id);
      if (!file) return;
      if (file.size !== null && file.size > PREVIEW_CAP) {
        settle(id, { state: "too-large" });
        return;
      }
      const asked = round;
      settle(id, { state: "loading" });
      read(entityId, id).then(
        (bytes) => {
          if (asked === round) settle(id, { state: "ready", src: `data:${bytes.mime};base64,${bytes.data}` });
        },
        (error: unknown) => {
          if (asked !== round) return;
          const message = isIpcError(error) ? error.message : String(error);
          // The source's own cap, for a file the list gave no size.
          if (isIpcError(error) && error.code === "invalid" && /\bpreviews an attachment up to\b/.test(message)) settle(id, { state: "too-large" });
          else settle(id, { state: "failed", reason: failureOf(error), message });
        },
      );
    },
    fetchImages() {
      for (const file of files) if (isImage(file)) previews.fetch(file.id);
    },
    load(id) {
      previews.fetch(id);
      return previews.when(id);
    },
    when(id) {
      return new Promise<string>((resolve, reject) => {
        const answer = (preview: Preview) => (preview.state === "ready" ? resolve(preview.src) : reject(preview));
        const now = peek(id);
        const listed = files.some((file) => file.id === id);
        if (listed && (now.state === "loading" || now.state === "none")) waiting.set(id, [...(waiting.get(id) ?? []), answer]);
        else answer(now);
      });
    },
    watch(node, id) {
      if (observe === null) {
        previews.fetch(id);
        return () => {};
      }
      const watcher = observe((entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        watcher.disconnect();
        previews.fetch(id);
      });
      watcher.observe(node);
      return () => watcher.disconnect();
    },
    retry() {
      round += 1;
      attempt += 1;
      const failed = [...states].filter(([, preview]) => preview.state === "failed").map(([id]) => id);
      for (const id of failed) states.delete(id);
      // Under way when the round moved: its answer is dropped, so ask again.
      const loading = [...states].filter(([, preview]) => preview.state === "loading").map(([id]) => id);
      for (const id of loading) states.delete(id);
      for (const id of [...failed, ...loading]) previews.fetch(id);
    },
    drop() {
      round += 1;
      states.clear();
      waiting.clear();
    },
  };
  return previews;
}
