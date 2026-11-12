/**
 * Reads a press would make, started while the pointer rests on its link (the inner-pages pass of PR #2479,
 * 2026-11-12): the first read past the mirror a repository or a pull request's *Files changed* waits for, so the
 * source's round trip runs while the hand is on its way to the press. A repository's landing reads its top directory
 * at once (the branch list and the last commit come from the mirror, `Source::mirror_listing`); a pull request's
 * *Files changed* its files. What it reads is the page's own read, kept by the backend (ADR-0042), so the page's ask
 * after the press is answered from it, or joins it while it is out (`readViewAhead`). Each is asked once a session, and only after the
 * pointer has stood on the link a moment, so a pointer swept across a list asks nothing.
 */
import { readViewAhead } from "../../ipc/entity";

/** How long the pointer stands on a link before its read starts. */
export const REST_MS = 80;

type Read = (entityId: string, section: string | null) => Promise<unknown>;

/** The read a link's address names a page's first wait for, or `null`: `#/repo/<id>`, `#/pr/<id>/diff`. */
export function readOf(href: string): { entityId: string; section: string | null } | null {
  const repo = /^#\/repo\/([^/]+)$/.exec(href);
  if (repo) return { entityId: decodeURIComponent(repo[1]!), section: "files" };
  const files = /^#\/pr\/([^/]+)\/diff$/.exec(href);
  if (files) return { entityId: decodeURIComponent(files[1]!), section: "files" };
  return null;
}

/** Listens on `root` for the pointer resting on such a link; the returned function stops it. */
export function installHoverReads(root: EventTarget = document, read: Read = readViewAhead, restMs = REST_MS): () => void {
  const asked = new Set<string>();
  let timer: ReturnType<typeof setTimeout> | null = null;
  let over: Element | null = null;
  const cancel = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };
  const onOver = (event: Event) => {
    const link = (event.target as Element | null)?.closest?.("a[href^='#/repo/'], a[href^='#/pr/']") ?? null;
    if (link === over) return;
    over = link;
    cancel();
    const ask = link === null ? null : readOf(link.getAttribute("href") ?? "");
    if (ask === null) return;
    const key = `${ask.entityId}\u0000${ask.section ?? ""}`;
    if (asked.has(key)) return;
    timer = setTimeout(() => {
      timer = null;
      asked.add(key);
      // A read that fails here fails again for the page, which says why; nothing is said twice.
      read(ask.entityId, ask.section).catch(() => {});
    }, restMs);
  };
  root.addEventListener("pointerover", onOver);
  return () => {
    cancel();
    root.removeEventListener("pointerover", onOver);
  };
}
