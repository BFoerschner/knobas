<!--
  A JAR in a comparison (2026-11-22, Fable's ruling; `docs/decisions/2026-11-22-a-jar-is-compared-by-what-it-holds.md`):
  in the rendered stage's one column, a head line and the entries that differ between the two sides, told by name, CRC
  and size from each side's listing (`render/jar-diff.ts`), with no entry read.

  - **The head line**: the artifact's name and version, its kind and its Java release, an arrow only where one differs.
  - **Rows**, flat by path, 29 px each as the JAR view's: the diff card's badge, the path, what it holds (a family's
    nested classes, a library's two versions), its sizes. A class and its nested classes are one row; a library across a
    version is one.
  - **A row opened** draws once its reads are in, in one frame, at its own height inside the stage's scroller, which
    holds the card's outside still; while it reads, its size's place says so. A changed text entry is its line diff, a
    changed image its two frames, a changed nested JAR its own list one indent in, a changed class its decompiled source
    diffed (or its outline: the caption's *Outline | Source*), an added or removed entry its one side as the JAR view
    draws it. At most 400 lines, then *Show the other N lines*.
  - **Decompiling** runs one class at a time app-wide, in press order (`queue`); only a press starts it. One stopped by
    another is asked once more, then the outline stands with the reason above it.
-->
<script module lang="ts">
  /** Decompiles one at a time, in press order: the backend stops a decompile when another starts. */
  let queue: Promise<unknown> = Promise.resolve();
  function inTurn<T>(job: () => Promise<T>): Promise<T> {
    const run = queue.then(job, job);
    queue = run.catch(() => {});
    return run;
  }
  /** Forgets the decompiles waiting their turn (tests: one test's held decompile never holds the next's). */
  export function forgetDecompileQueue() {
    queue = Promise.resolve();
  }
</script>

<script lang="ts">
  import "../css/jar.css";

  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  import { ipcErrorMessage } from "../../ipc";
  import type { JavaRuntime } from "../../ipc/app";
  import type { ArchiveIndex, ClassOutline, Hunk } from "../../ipc/entity";
  import { imageType, typedImageBlob } from "../../render/image";
  import { bytes, textOf } from "../../render/jar";
  import { compareJars, firstLines, headLine, outlineText, type JarCompare, type JarRow } from "../../render/jar-diff";
  import type { ArchivePorts } from "../../render/jar-ports";
  import { renderPort, type RenderPort } from "../../render/port";
  import { Windowed } from "../../shell/windowed.svelte";
  import TfJarClass from "../render/TfJarClass.svelte";
  import TfJarCode from "../render/TfJarCode.svelte";
  import TfPath from "../TfPath.svelte";
  import type { ClassMode } from "./class-mode.svelte";
  import { lineDiff } from "./line-diff";
  import type { Side } from "./rendered";
  import { expandLazy, isAltClick, LAZY_FETCH_CAP, TREE_EXPAND_CAP } from "../tree-alt";

  let {
    path,
    before,
    after,
    compare,
    archives,
    java,
    mode,
    reopen = () => Promise.resolve(null),
    render = renderPort(),
  }: {
    path: string;
    /** Each side's listing; `null` where the file is not on that side. */
    before: ArchiveIndex | null;
    after: ArchiveIndex | null;
    compare: JarCompare;
    archives: Omit<ArchivePorts, "open">;
    /** What decompiling stands at; `null` while it is looked for. */
    java: JavaRuntime | null;
    mode: ClassMode;
    /** A side's archive listed again, where the backend let it go (it keeps the last eight). */
    reopen?: (side: Side) => Promise<ArchiveIndex | null>;
    render?: RenderPort;
  } = $props();

  const fileName = $derived(path.slice(path.lastIndexOf("/") + 1));
  const line = $derived(headLine(before, after, fileName));
  /** The most lines an opened row draws before *Show the other N lines*. */
  const LINES = 400;
  /** The most bytes a side of a text entry may be to be diffed as lines. */
  const TEXT_CAP = 2 * 1024 * 1024;

  /** Where each scope's entries are on each side: `""` the archive, a nested one by its row's key. */
  type Nested = { ok: true; compare: JarCompare; paths: { old: string | null; new: string | null } } | { ok: false; error: string };
  const nested = new SvelteMap<string, Nested>();
  /** Each side's handle per scope, `side|scope`. Not reactive. */
  const handles = new Map<string, string>();

  /** What an opened row shows, by its key (a changed class's by its key and the mode it was read in). */
  type Content =
    | { kind: "hunks"; hunks: Hunk[]; note: string | null; same: string }
    | { kind: "blocks"; blocks: { name: string; hunks: Hunk[] }[]; note: string | null; same: string }
    | { kind: "code"; text: string; at: string }
    | { kind: "class"; side: Side; outline: ClassOutline; at: string }
    | { kind: "images"; old: string | null; new: string | null }
    | { kind: "note"; text: string };
  const contents = new SvelteMap<string, Content>();
  const open = new SvelteSet<string>();
  /** Rows being read, and what their size's place says meanwhile. */
  const reading = new SvelteMap<string, string>();
  /** Rows whose lines past the first 400 are shown. */
  const whole = new SvelteSet<string>();

  $effect(() => {
    void before;
    void after;
    handles.clear();
    if (before !== null) handles.set("old|", before.handle);
    if (after !== null) handles.set("new|", after.handle);
    return () => {
      for (const c of contents.values()) if (c.kind === "images") for (const url of [c.old, c.new]) if (url !== null) URL.revokeObjectURL(url);
    };
  });

  type Drawn =
    | { k: string; kind: "row"; depth: number; scope: string; row: JarRow; open: boolean }
    | { k: string; kind: "detail"; depth: number; scope: string; row: JarRow }
    | { k: string; kind: "note"; depth: number; text: string };

  const keyOf = (scope: string, row: JarRow) => `${scope}|${row.key}`;
  const scopeOf = (scope: string, row: JarRow) => (scope === "" ? row.key : `${scope}!${row.key}`);

  const rows = $derived.by(() => {
    const out: Drawn[] = [];
    const openNow = new Set(open);
    const walk = (c: JarCompare, scope: string, depth: number) => {
      for (const row of c.rows) {
        const k = keyOf(scope, row);
        const isOpen = openNow.has(k);
        out.push({ k: `r${k}`, kind: "row", depth, scope, row, open: isOpen });
        if (!isOpen) continue;
        if (row.kind === "archive") {
          const inner = nested.get(scopeOf(scope, row));
          if (inner === undefined) continue;
          if (!inner.ok) out.push({ k: `n${k}`, kind: "note", depth: depth + 1, text: inner.error });
          else if (inner.compare.rows.length === 0) out.push({ k: `n${k}`, kind: "note", depth: depth + 1, text: "Every entry inside is the same on both sides." });
          else walk(inner.compare, scopeOf(scope, row), depth + 1);
        } else out.push({ k: `d${k}`, kind: "detail", depth, scope, row });
      }
    };
    walk(compare, "", 0);
    return out;
  });

  const win = new Windowed<string>({
    count: () => rows.length,
    key: (i) => rows[i]!.k,
    estimate: 29,
    estimateOf: (k) => (k.startsWith("d") ? 240 : 29),
  });

  function gone(error: unknown): boolean {
    return typeof error === "object" && error !== null && (error as { code?: string }).code === "not_found" && ipcErrorMessage(error).includes("no longer open");
  }

  /** `side`'s handle for `scope`, opening the nested archives down to it as needed. */
  async function handleOf(side: Side, scope: string): Promise<string> {
    const known = handles.get(`${side}|${scope}`);
    if (known !== undefined) return known;
    if (scope === "") {
      const index = await reopen(side);
      if (index === null) throw new Error(`the archive ${side === "old" ? "before" : "after"} the change is not there`);
      handles.set(`${side}|`, index.handle);
      return index.handle;
    }
    const cut = scope.lastIndexOf("!");
    const parent = cut < 0 ? "" : scope.slice(0, cut);
    const inner = nested.get(scope);
    const entry = inner !== undefined && inner.ok ? inner.paths[side] : null;
    if (entry === null) throw new Error("that archive is not on this side");
    const index = await archives.nested(await handleOf(side, parent), entry);
    handles.set(`${side}|${scope}`, index.handle);
    return index.handle;
  }

  /** A read on `side`'s archive at `scope`, opened again once if the backend let it go. */
  async function onArchive<T>(side: Side, scope: string, work: (handle: string) => Promise<T>): Promise<T> {
    try {
      return await work(await handleOf(side, scope));
    } catch (error) {
      if (!gone(error)) throw error;
      handles.clear();
      return work(await handleOf(side, scope));
    }
  }

  /** A text less its last line's newline: no empty last line is diffed. */
  const endless = (text: string) => text.replace(/\r?\n$/, "");
  const entryOf = (row: JarRow, side: Side) => (side === "old" ? row.old : row.new);

  async function openNested(scope: string, row: JarRow) {
    const inner = scopeOf(scope, row);
    const paths = { old: row.old?.path ?? null, new: row.new?.path ?? null };
    const list = async (side: Side) => {
      const at = paths[side];
      return at === null ? null : onArchive(side, scope, (h) => archives.nested(h, at));
    };
    try {
      const [a, b] = [await list("old"), await list("new")];
      if (a !== null) handles.set(`old|${inner}`, a.handle);
      if (b !== null) handles.set(`new|${inner}`, b.handle);
      nested.set(inner, { ok: true, compare: compareJars(a, b), paths });
    } catch (error) {
      nested.set(inner, { ok: false, error: ipcErrorMessage(error) });
    }
  }

  /** The decompiled top-level class on `side`, asked once more where another decompile stopped it. */
  async function decompiled(side: Side, scope: string, entry: string): Promise<string> {
    const once = () => inTurn(() => onArchive(side, scope, (h) => archives.decompile(h, entry)));
    try {
      return await once();
    } catch (error) {
      if (!ipcErrorMessage(error).includes("another class was decompiled")) throw error;
      return once();
    }
  }

  async function outlines(scope: string, row: JarRow): Promise<Extract<Content, { kind: "blocks" }>> {
    const names = [...new Set([...(row.old?.members ?? []), ...(row.new?.members ?? [])])];
    const blocks: { name: string; hunks: Hunk[] }[] = [];
    for (const name of names) {
      const text = async (side: Side) => {
        const has = entryOf(row, side)?.members.includes(name) ?? false;
        return has ? outlineText(await onArchive(side, scope, (h) => archives.cls(h, name))) : "";
      };
      const diff = lineDiff(name, await text("old"), await text("new"));
      if (diff.hunks.length > 0) blocks.push({ name: name.slice(name.lastIndexOf("/") + 1).replace(/\.class$/, ""), hunks: diff.hunks });
    }
    return { kind: "blocks", blocks, note: null, same: "Its outline is the same on both sides: the change is inside its methods." };
  }

  async function readClass(scope: string, row: JarRow, how: ClassMode): Promise<Content> {
    if (how === "source") {
      try {
        reading.set(keyOf(scope, row), "Decompiling…");
        const [a, b] = [await decompiled("old", scope, row.old!.path), await decompiled("new", scope, row.new!.path)];
        const diff = lineDiff(row.key, endless(a), endless(b));
        return {
          kind: "hunks",
          hunks: diff.hunks,
          note: `Decompiled by ${java?.decompiler ?? "the decompiler"} on both sides: not the original source.`,
          same: "The decompiled source is the same on both sides.",
        };
      } catch (error) {
        const fallback = await outlines(scope, row);
        return { ...fallback, note: `Could not decompile it (${ipcErrorMessage(error)}): its outline instead.` };
      }
    }
    return outlines(scope, row);
  }

  async function readEntry(scope: string, row: JarRow): Promise<Content> {
    const kind = row.kind;
    const one: Side | null = row.change === "added" ? "new" : row.change === "removed" ? "old" : null;
    if (kind === "class") {
      if (one !== null) {
        const at = entryOf(row, one)!.path;
        return { kind: "class", side: one, outline: await onArchive(one, scope, (h) => archives.cls(h, at)), at };
      }
      const how: ClassMode = java?.can_decompile === true ? mode : "outline";
      return readClass(scope, row, how);
    }
    const read = async (side: Side) => {
      const at = entryOf(row, side)?.path ?? null;
      return at === null ? null : onArchive(side, scope, (h) => archives.entry(h, at));
    };
    if (kind === "image") {
      const url = async (side: Side) => {
        const got = await read(side);
        return got === null ? null : URL.createObjectURL(typedImageBlob(got, imageType(row.key)));
      };
      return { kind: "images", old: await url("old"), new: await url("new") };
    }
    const [a, b] = [await read("old"), await read("new")];
    const ta = a === null || a.length > TEXT_CAP ? null : textOf(a);
    const tb = b === null || b.length > TEXT_CAP ? null : textOf(b);
    if (one !== null) {
      const text = one === "old" ? ta : tb;
      const size = (one === "old" ? a : b)?.length ?? 0;
      return text === null ? { kind: "note", text: `A binary entry of ${bytes(size)}.` } : { kind: "code", text, at: row.key };
    }
    if (ta !== null && tb !== null) {
      return { kind: "hunks", hunks: lineDiff(row.key, endless(ta), endless(tb)).hunks, note: null, same: "Its text is the same on both sides." };
    }
    return { kind: "note", text: `A binary entry: ${bytes(a?.length ?? 0)} before, ${bytes(b?.length ?? 0)} after.` };
  }

  const contentKey = (k: string, row: JarRow) => (row.kind === "class" && row.change === "changed" ? `${k}|${java?.can_decompile === true ? mode : "outline"}` : k);

  async function toggle(scope: string, row: JarRow, all = false) {
    const k = keyOf(scope, row);
    if (open.has(k)) {
      open.delete(k);
      if (all && row.kind === "archive") {
        const inner = scopeOf(scope, row);
        for (const key of [...open]) if (key.startsWith(`${inner}|`) || key.startsWith(`${inner}!`)) open.delete(key);
      }
      return;
    }
    if (reading.has(k)) return;
    if (row.kind === "archive") {
      if (!all) {
        if (!nested.has(scopeOf(scope, row))) {
          reading.set(k, "Reading…");
          await openNested(scope, row);
          reading.delete(k);
        }
        open.add(k);
        return;
      }
      const byKey = new Map<string, { scope: string; row: JarRow }>([[k, { scope, row }]]);
      const archiveChildren = (key: string): string[] | undefined => {
        const item = byKey.get(key);
        if (item === undefined) return undefined;
        const innerScope = scopeOf(item.scope, item.row);
        const got = nested.get(innerScope);
        if (got === undefined) return undefined;
        if (!got.ok) return [];
        const kids: string[] = [];
        for (const child of got.compare.rows) {
          if (child.kind !== "archive") continue;
          const ck = keyOf(innerScope, child);
          byKey.set(ck, { scope: innerScope, row: child });
          kids.push(ck);
        }
        return kids;
      };
      await expandLazy(k, {
        nodeCap: TREE_EXPAND_CAP,
        fetchCap: LAZY_FETCH_CAP,
        cached: archiveChildren,
        fetch: async (key) => {
          const item = byKey.get(key);
          if (item === undefined || reading.has(key)) return undefined;
          reading.set(key, "Reading…");
          await openNested(item.scope, item.row);
          reading.delete(key);
          return archiveChildren(key);
        },
        open: (key) => {
          open.add(key);
        },
      });
      return;
    }
    await fill(scope, row);
    open.add(k);
  }

  async function fill(scope: string, row: JarRow) {
    const k = keyOf(scope, row);
    const at = contentKey(k, row);
    if (contents.has(at)) return;
    reading.set(k, "Reading…");
    try {
      contents.set(at, await readEntry(scope, row));
    } catch (error) {
      contents.set(at, { kind: "note", text: ipcErrorMessage(error) });
    } finally {
      reading.delete(k);
    }
  }

  // The caption's *Outline | Source* pressed: each changed class open reads in the new mode (the press starts it).
  let seenMode: ClassMode | null = null;
  $effect(() => {
    const now = mode;
    if (seenMode === null || seenMode === now) {
      seenMode = now;
      return;
    }
    seenMode = now;
    for (const d of rows) if (d.kind === "detail" && d.row.kind === "class" && d.row.change === "changed") void fill(d.scope, d.row);
  });

  const BADGE = { added: "A", removed: "D", changed: "M" } as const;
  function sizes(row: JarRow): string {
    if (row.change === "added") return bytes(row.new!.size);
    if (row.change === "removed") return bytes(row.old!.size);
    return row.old!.size === row.new!.size ? bytes(row.new!.size) : `${bytes(row.old!.size)} → ${bytes(row.new!.size)}`;
  }
  function meta(row: JarRow): string {
    if (row.lib !== null) return `${row.lib.from ?? "?"} → ${row.lib.to ?? "?"}`;
    const side = row.new ?? row.old!;
    const extra = side.members.length - (side.members.includes(row.key) ? 1 : 0);
    if (row.kind === "class" && extra > 0) return `and ${extra} nested ${extra === 1 ? "class" : "classes"}`;
    return "";
  }
  /** A row's indent, through the CSSOM (CSP refuses inline styles). */
  function indent(depth: number) {
    return (el: HTMLElement) => {
      el.style.setProperty("--jar-depth", String(depth));
    };
  }
  const SIGN = { add: "+", del: "−", context: " " } as const;
</script>

{#snippet hunksOf(k: string, hunks: Hunk[])}
  {@const cut = whole.has(k) ? { hunks, more: 0 } : firstLines(hunks, LINES)}
  {#each cut.hunks as hunk, h (h)}
    <div class="tf-pdfd-hunk">
      {#each hunk.lines as l, n (n)}
        <div class="tf-ln" class:tf-add={l.kind === "add"} class:tf-del={l.kind === "del"}>
          <span class="tf-no">{l.old ?? ""}</span><span class="tf-no">{l.new ?? ""}</span><span>{SIGN[l.kind]} {l.text}</span>
        </div>
      {/each}
    </div>
  {/each}
  {#if cut.more > 0}
    <p class="tf-jar-more"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => whole.add(k)}>Show the other {cut.more.toLocaleString("en")} lines</button></p>
  {/if}
{/snippet}

<div class="tf-jdiff">
  <p class="tf-jdiff-head" title={line}>{line}</p>
  {#if compare.rows.length === 0}
    <p class="tf-jar-none tf-jdiff-same">Every entry is the same on both sides: the archives differ only in how they are packed.</p>
  {/if}
  <div class="tf-jar-list" {@attach win.list}>
    {#each win.slice(rows) as d (d.k)}
      {#if d.kind === "row"}
        {@const k = keyOf(d.scope, d.row)}
        <div class="tf-jar-row" data-k={d.k} {@attach win.row(d.k)} {@attach indent(d.depth)}>
          <button type="button" class="tf-jar-rbtn tf-jdiff-row" aria-expanded={d.open} onclick={(event) => void toggle(d.scope, d.row, isAltClick(event))}>
            <span class="tf-bd tf-bd-{BADGE[d.row.change]}" title={d.row.change}>{BADGE[d.row.change]}</span>
            <span class="tf-jar-fname" class:tf-jar-class={d.row.kind === "class"}><TfPath path={d.row.key} /></span>
            <span class="tf-jar-meta">{meta(d.row)}</span>
            <span class="tf-jar-size">{reading.get(k) ?? sizes(d.row)}</span>
          </button>
        </div>
      {:else if d.kind === "note"}
        <div class="tf-jar-row tf-jar-rnote" data-k={d.k} {@attach win.row(d.k)} {@attach indent(d.depth)}><p>{d.text}</p></div>
      {:else}
        {@const k = keyOf(d.scope, d.row)}
        {@const c = contents.get(contentKey(k, d.row)) ?? contents.get(k)}
        <div class="tf-jar-detail" data-k={d.k} {@attach win.row(d.k)} {@attach indent(d.depth)}>
          {#if c === undefined}
            <!-- Read before it opens: nothing to draw here. -->
          {:else if c.kind === "hunks"}
            {#if c.note !== null}<p class="tf-jar-dnote">{c.note}</p>{/if}
            {#if c.hunks.length === 0}<p class="tf-jar-dnote">{c.same}</p>{:else}{@render hunksOf(k, c.hunks)}{/if}
          {:else if c.kind === "blocks"}
            {#if c.note !== null}<p class="tf-jar-dnote">{c.note}</p>{/if}
            {#if c.blocks.length === 0}<p class="tf-jar-dnote">{c.same}</p>{/if}
            {#each c.blocks as b (b.name)}
              <p class="tf-jdiff-cls"><code>{b.name}</code></p>
              {@render hunksOf(`${k}|${b.name}`, b.hunks)}
            {/each}
          {:else if c.kind === "code"}
            <TfJarCode text={c.text} path={c.at} {render} />
          {:else if c.kind === "class"}
            <TfJarClass outline={c.outline} path={c.at} java={null} source={null} mode="outline" onmode={() => {}} {render} />
          {:else if c.kind === "images"}
            <div class="tf-jdiff-imgs">
              {#each [["before", c.old], ["after", c.new]] as [label, url] (label)}
                {#if url !== null}<figure class="tf-jar-img"><img src={url} alt="{d.row.key} {label}" /><figcaption>{label}</figcaption></figure>{/if}
              {/each}
            </div>
          {:else}
            <p class="tf-jar-dnote">{c.text}</p>
          {/if}
        </div>
      {/if}
    {/each}
  </div>
</div>
