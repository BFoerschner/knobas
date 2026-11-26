/**
 * A Docs page's reads, asked at the press on its link (Björn, 2026-11-25: the old page stayed until the new one was
 * read; Fable's ruling, `docs/decisions/2026-11-25-a-docs-page-opens-at-once-and-a-site-leaves-where-it-is.md`). A
 * page's reads go with the press, as a ticket's and a repository's do (`job/press-reads.ts`, `repo/hover-reads.ts`):
 * the page's text (`docs_page`) and its row (`entity_head`), each the call the page makes with its arguments, so the
 * page's own asks join them in the answer cache (`ipc/answer-cache.ts`), on their way or kept. No visit: the page
 * records that once it is drawn. A primary press or `Enter` on any `#/page/<id>` link, wherever it is: a Docs row, a
 * link in a page, a ticket's Pages, the inbox.
 */
import { docsPage, entityHead } from "../../ipc/entity";

/** How long one press's asks stand for the next: a press and its click ask once. */
export const ASKED_MS = 3000;

export interface PageReads {
  page: (entityId: string) => Promise<unknown>;
  head: (entityId: string) => Promise<unknown>;
}

// Called through, not held: a module mock short of these exports (the shell's tests) stays importable.
const BRIDGE: PageReads = { page: (id) => docsPage(id), head: (id) => entityHead(id) };

/** When each page's reads were last asked, by id. */
const asked = new Map<string, number>();

/** Asks the reads page `entityId` waits on, once per {@link ASKED_MS}. */
export function readDocsPageAhead(entityId: string, reads: PageReads = BRIDGE, now = Date.now()): void {
  const last = asked.get(entityId);
  if (last !== undefined && now - last < ASKED_MS) return;
  asked.set(entityId, now);
  // A read that fails here fails again for the page, which says why; nothing is said twice.
  void reads.page(entityId).catch(() => {});
  void reads.head(entityId).catch(() => {});
}

/** The page a link's address opens (`#/page/<id>[/<section>|/mention]`), else `null`. */
export function docsPageOf(href: string): string | null {
  const at = /^#\/page\/([^/]+)(?:\/[^/]*)?$/.exec(href);
  if (!at) return null;
  try {
    return decodeURIComponent(at[1]!);
  } catch {
    return null;
  }
}

/** Listens on `root` for a press on a Docs page's link; the returned function stops it. */
export function installDocsPressReads(root: EventTarget = document, read: (entityId: string) => void = (id) => readDocsPageAhead(id)): () => void {
  const onDown = (event: Event) => {
    if (event.type === "keydown" && (event as KeyboardEvent).key !== "Enter") return;
    if (event.type === "pointerdown" && (event as PointerEvent).button !== 0) return;
    const link = (event.target as Element | null)?.closest?.("a[href^='#/page/']") ?? null;
    const id = link === null ? null : docsPageOf(link.getAttribute("href") ?? "");
    if (id !== null) read(id);
  };
  root.addEventListener("pointerdown", onDown, { capture: true, passive: true });
  root.addEventListener("keydown", onDown, { capture: true, passive: true });
  return () => {
    root.removeEventListener("pointerdown", onDown, { capture: true });
    root.removeEventListener("keydown", onDown, { capture: true });
  };
}

/** Tests only: no page asked. */
export function forgetDocsPressReads(): void {
  asked.clear();
}
