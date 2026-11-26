/**
 * **Compile the start's code at the start** (the ux30 Windows pass, 2026-11-23,
 * `docs/decisions/2026-11-23-the-start-compiles-its-code.md`). V8 compiles a function the first time it is called,
 * and the window's code cache keeps only the functions compiled by the time the page's first run ended. So the code a
 * page runs first when it is first opened, a repository's landing, a run, a Docs page, was compiled at that press,
 * every start: 86 ms of a repository's first open at CPU 4x in knobas's own WebView2 window on Björn's PC.
 *
 * Chromium's explicit compile hint, `//# allFunctionsCalledOnLoad` as a script's first line, compiles every function
 * of that script when it loads (off the main thread while it streams), so the cache holds all of them and a later
 * start finds that code compiled. On every chunk of the start (2.5 MB) it made each start 100 ms slower on the PC,
 * since every start then reads the whole compiled code back from the cache. So the pages whose first open compiled
 * the most are a chunk of their own, {@link FIRST_OPENS} (`vite.config.ts`: a repository, a ticket and Tickets, a
 * run and Builds, Docs), still loaded at the start, and only that chunk is hinted. A chunk loaded on demand (a
 * grammar, D2, pdf.js) is never hinted.
 */

/** The chunk of the pages whose first open compiled the most, named in `vite.config.ts`'s `codeSplitting`. */
export const FIRST_OPENS = "first-opens";

/** The modules {@link FIRST_OPENS} holds: what a repository, a ticket, Tickets, a run, Builds and Docs draw. */
export const FIRST_OPENS_MODULES = /src\/lib\/tf\/(repo|docs|builds|job|tickets)\//;

/** The hint: Chromium reads it only as the script's first line. */
export const COMPILE_HINT = "//# allFunctionsCalledOnLoad\n";

/** What the hint needs of a bundle's output: Rollup's and Rolldown's chunks have these fields. */
export interface HintableChunk {
  type: "chunk" | "asset";
  isEntry?: boolean;
  name?: string;
  imports?: string[];
  code?: string;
}

/** `code` with the hint as its first line, once. */
export function withHint(code: string): string {
  return code.startsWith(COMPILE_HINT) ? code : COMPILE_HINT + code;
}

/**
 * The chunks named `hinted` that the entry named `entry` loads at the start (itself or a static import of it, at any
 * depth), each one's `code`, where it has one, given the hint once. Returns their file names, sorted.
 */
export function hintStartChunks(bundle: Record<string, HintableChunk>, hinted = FIRST_OPENS, entry = "index"): string[] {
  const start = Object.entries(bundle).find(([, chunk]) => chunk.type === "chunk" && chunk.isEntry && chunk.name === entry);
  if (start === undefined) return [];
  const seen = new Set<string>();
  const queue = [start[0]];
  while (queue.length > 0) {
    const file = queue.pop()!;
    const chunk = bundle[file];
    if (seen.has(file) || chunk === undefined || chunk.type !== "chunk") continue;
    seen.add(file);
    queue.push(...(chunk.imports ?? []));
  }
  const picked = [...seen].filter((file) => bundle[file]!.name === hinted).sort();
  for (const file of picked) {
    const chunk = bundle[file]!;
    if (chunk.code !== undefined) chunk.code = withHint(chunk.code);
  }
  return picked;
}
