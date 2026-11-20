/**
 * Reads a press would make, started while the pointer rests on its link (the inner-pages pass of PR #2479,
 * 2026-11-12): the first read past the mirror a repository or a pull request's *Files changed* waits for, so the
 * source's round trip runs while the hand is on its way to the press. A repository's landing reads its top directory
 * at once (the branch list and the last commit come from the mirror, `Source::mirror_listing`); a pull request's
 * *Files changed* its files. What it reads is the page's own read, kept by the backend (ADR-0042), so the page's ask
 * after the press is answered from it, or joins it while it is out (`readViewAhead`). Each is asked once a session, and only after the
 * pointer has stood on the link a moment, so a pointer swept across a list asks nothing.
 */
import { codeFacts, getEntity, readViewAhead, repoBranchPulls, repoPipelines } from "../../ipc/entity";
import { likelySections } from "./repo";

/** The file a landing that names none opens first, its README, as the Files tab guesses it (`TfRepoFiles.svelte`). */
export const LANDING_README = "README.md";

/** The README guess a press asked, by entity: the section. */
const guessed = new Map<string, string>();

/**
 * The section a press on the repository's link asked of `entityId` as its README guess ({@link readLandingAhead}), or
 * `null`: the landing takes it even where its top listing came first.
 */
export function landingGuess(entityId: string): string | null {
  return guessed.get(entityId) ?? null;
}

/**
 * How long a press's reads are kept after they land, for the page's own asks to take: a repository's landing at CPU 6x
 * asked them 170 ms after the press, once it was built.
 */
export const PRESS_KEEP_MS = 3000;

/**
 * The reads a repository's landing makes, started at the press on its link, before the page is built (the ux14 pass,
 * 2026-11-16): at CPU 4x and 6x the landing sent them 118 and 166 ms after the press, behind the build of the page,
 * and its first open waited for them after that. The page's own asks take them: the mirror's reads through the answer
 * cache (`ipc/answer-cache.ts`, in flight or kept), its views from {@link readViewAhead}. Each is the call the page
 * makes, with the same arguments: `TfRepo.svelte`'s header, tabs and counts, the Files tab's top listing.
 */
export function readLandingAhead(entityId: string): void {
  // A press and the address it sets ask once: the second joins the first's reads (the compare-first-open pass).
  const last = landingAsked.get(entityId);
  if (last !== undefined && Date.now() - last < PRESS_KEEP_MS) return;
  landingAsked.set(entityId, Date.now());
  const quiet = (read: Promise<unknown>) => void read.catch(() => {});
  // The README guess as the landing makes it (the ux16 overs pass, 2026-11-17): asked once the record names the source's
  // tree sections, which comes back before the page's build at 6x, so the README is not a second round after it. On
  // Björn's PC the landing asked it once its listing had come, behind the build, and waited 130 ms more at 6x.
  quiet(
    getEntity(entityId).then((detail) => {
      const likely = likelySections(LANDING_README, detail.tree ?? null);
      if (likely === null) return;
      guessed.set(entityId, likely.file);
      quiet(readViewAhead(entityId, likely.file, PRESS_KEEP_MS));
    }),
  );
  quiet(codeFacts([entityId]));
  quiet(repoBranchPulls(entityId));
  quiet(repoPipelines(entityId));
  for (const section of ["files", "commits", "branches"]) quiet(readViewAhead(entityId, section, PRESS_KEEP_MS));
}

/** When each landing's reads were last asked, by entity. */
const landingAsked = new Map<string, number>();
/** The repository pages drawn now, shown or kept hidden, by entity: how many. */
const pagesHeld = new Map<string, number>();

/** A repository's page is drawn for `entityId` (shown or kept): its landing reads for itself. Returns its release. */
export function holdRepoPage(entityId: string): () => void {
  pagesHeld.set(entityId, (pagesHeld.get(entityId) ?? 0) + 1);
  return () => {
    const n = (pagesHeld.get(entityId) ?? 1) - 1;
    if (n > 0) pagesHeld.set(entityId, n);
    else pagesHeld.delete(entityId);
  };
}

/**
 * The address moved to a repository's landing no page holds (a launcher's jump, a link set by a page, history): its
 * reads start now, before the page is built, as a press on its link starts them (Fable's ruling, the compare-first-open
 * pass, 2026-11-20). Called by the shell as the address changes. A page kept for it reads again as it is shown.
 */
export function readLandingAtAddress(hash: string): void {
  const id = landingOf(hash);
  if (id !== null && !pagesHeld.has(id)) readLandingAhead(id);
}

/** Tests only: no landing asked, no page held. */
export function forgetLandingReads(): void {
  landingAsked.clear();
  pagesHeld.clear();
  guessed.clear();
}

/** A repository's landing that `href` names (`#/repo/<id>`), else `null`. */
function landingOf(href: string): string | null {
  const repo = /^#\/repo\/([^/]+)$/.exec(href);
  if (!repo) return null;
  try {
    return decodeURIComponent(repo[1]!);
  } catch {
    return null;
  }
}

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
export function installHoverReads(
  root: EventTarget = document,
  read: Read = readViewAhead,
  restMs = REST_MS,
  press: ((entityId: string) => void) | null = readLandingAhead,
): () => void {
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
  // A press before the rest is over makes the page's own read: the rest's would go out beside it, the source asked
  // twice (the WSL block, 2026-11-14: a pull request of 3,000 files read its 8 MB from Gitea twice at once).
  const onDown = (event: Event) => {
    if (event.type === "keydown" && (event as KeyboardEvent).key !== "Enter") return;
    const link = (event.target as Element | null)?.closest?.("a[href^='#/repo/'], a[href^='#/pr/']") ?? null;
    const href = link?.getAttribute("href") ?? "";
    const ask = link === null ? null : readOf(href);
    if (ask === null) return;
    cancel();
    const key = `${ask.entityId}\u0000${ask.section ?? ""}`;
    // A repository's landing is read at the press, all of it: a top the rest read already is joined or held.
    const landing = landingOf(href);
    if (landing !== null && press !== null) press(landing);
    asked.add(key);
  };
  root.addEventListener("pointerover", onOver);
  root.addEventListener("pointerdown", onDown);
  root.addEventListener("keydown", onDown);
  return () => {
    cancel();
    root.removeEventListener("pointerover", onOver);
    root.removeEventListener("pointerdown", onDown);
    root.removeEventListener("keydown", onDown);
  };
}
