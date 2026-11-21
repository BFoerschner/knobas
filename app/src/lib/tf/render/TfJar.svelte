<!--
  A Java archive on the Code page (ux19, `docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md`): a `.jar`,
  `.war`, `.ear` or `.aar` drawn as what it holds. The file's renderer by default; it has no *Source* (it is a
  binary), so the head offers *Save to Downloads* beside the page's *Open in…*.

  - **The head**: the artifact's name and its version, the version in the method column's narrow heavy cut, the one
    loud thing; its Maven coordinates; what kind of archive; then the facts (the Java release it is compiled for, a
    multi-release JAR's releases, the main or application class, who built it with which JDK), its sizes packed and
    unpacked, and the whole manifest, its class path, other Maven coordinates and what the reading passed over in
    folds.
  - **The band**, stuck to the top of the page's scroller: *Filter* (every word matched against the entry's path),
    the count, and *Contents* / *Dependencies* where the archive holds JARs (`BOOT-INF/lib/`, `WEB-INF/lib/`).
  - **Contents**: the tree, folders with one folder in them drawn as one row (`com/example/demo`), each row its size;
    a class opens in place to its outline (`TfJarClass`), a resource to the file viewer's drawing (text highlighted,
    Markdown, an image, a PDF), a nested JAR to its own tree one level in. A filter lists every entry that matches
    by its path, the open nested JARs' too.
  - **Dependencies**: the nested JARs by name, their versions in a column (from their file names), each opening to
    its tree.

  The archive is read and listed in the backend (`archive_open`); an entry is read only when opened, and opens once
  its read is in, so it draws whole. The list is windowed: 10,000 entries draw what the screen shows.
-->
<script module lang="ts">
  import type { ArchiveIndex, ClassOutline } from "../../ipc/entity";
  import type { JavaRuntime } from "../../ipc/app";

  /** The archive reads the view makes (`archive_*`): the bridge's, a fake in tests. */
  export interface JarPorts {
    /** The archive this view draws, listed. */
    open: () => Promise<ArchiveIndex>;
    nested: (handle: string, entry: string) => Promise<ArchiveIndex>;
    entry: (handle: string, entry: string) => Promise<Uint8Array>;
    cls: (handle: string, entry: string) => Promise<ClassOutline>;
    decompile: (handle: string, entry: string) => Promise<string>;
    download: (handle: string, entry: string | null) => Promise<string>;
    java: () => Promise<JavaRuntime>;
  }


  /** A row's indent, one step a level, through the CSSOM (CSP refuses inline styles). */
  function indent(depth: number) {
    return (el: HTMLElement) => {
      el.style.setProperty("--jar-depth", String(depth));
    };
  }
</script>

<script lang="ts">
  import "../css/jar.css";

  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  import { ipcErrorMessage } from "../../ipc";
  import { Windowed } from "../../shell/windowed.svelte";
  import { filterKeys } from "../filter-keys";
  import { imageType, typedImageBlob } from "../../render/image";
  import {
    DUPLICATE,
    UNREADABLE,
    UNSAFE,
    bytes,
    filteredRows,
    head,
    kindOf,
    libRows,
    listing,
    scopeOf,
    terms,
    textOf,
    treeRows,
    type Listing,
    type Nested,
    type Row,
  } from "../../render/jar";
  import { renderPort, type RenderPort } from "../../render/port";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import TfPdf from "./TfPdf.svelte";
  import TfJarClass from "./TfJarClass.svelte";
  import TfJarCode from "./TfJarCode.svelte";

  let {
    path,
    ports,
    render = renderPort(),
  }: {
    /** The archive's path in the repository. */
    path: string;
    ports: JarPorts;
    render?: RenderPort;
  } = $props();

  const fileName = $derived(path.slice(path.lastIndexOf("/") + 1));

  type Opened = { ok: true; index: ArchiveIndex; listing: Listing } | { ok: false; error: string };
  let top = $state<Opened | null>(null);
  let slow = $state(false);
  let java = $state<JavaRuntime | null>(null);

  /** Each scope's handle: `""` the archive, a nested one's by its entry names from the top. Not reactive. */
  const handles = new Map<string, string>();
  const nested = new SvelteMap<string, Nested>();
  const openDirs = new SvelteSet<string>();
  const openFiles = new SvelteSet<string>();

  /** What an opened entry shows, by `scope|path`. */
  type Content =
    | { kind: "class"; outline: ClassOutline }
    | { kind: "text"; text: string }
    | { kind: "markdown"; text: string }
    | { kind: "image"; url: string }
    | { kind: "pdf"; bytes: Uint8Array }
    | { kind: "binary"; size: number }
    | { kind: "note"; text: string };
  const contents = new SvelteMap<string, Content>();
  /** Entries being read: their row says so in its size's place. */
  const reading = new SvelteSet<string>();
  const modes = new SvelteMap<string, "outline" | "source">();
  const sources = new SvelteMap<string, { text: string } | { error: string } | "reading">();
  let saved = $state<{ text: string; failed: boolean } | null>(null);

  $effect(() => {
    const read = ports;
    void path;
    let live = true;
    top = null;
    slow = false;
    handles.clear();
    nested.clear();
    openDirs.clear();
    openFiles.clear();
    contents.clear();
    reading.clear();
    modes.clear();
    sources.clear();
    saved = null;
    typed = "";
    tab = "contents";
    const timer = setTimeout(() => live && (slow = true), 200);
    read.open().then(
      (index) => {
        if (!live) return;
        clearTimeout(timer);
        handles.set("", index.handle);
        const l = listing(index);
        // One folder at the top (a WAR's `WEB-INF`, a library's `com`) opens: there is nothing else to pick.
        if (l.tree.dirs.length === 1 && l.tree.files.length === 0) openDirs.add(`|${l.tree.dirs[0]!.path}`);
        top = { ok: true, index, listing: l };
      },
      (error: unknown) => {
        if (!live) return;
        clearTimeout(timer);
        top = { ok: false, error: ipcErrorMessage(error) };
      },
    );
    read.java().then(
      (rt) => live && (java = rt),
      () => {},
    );
    return () => {
      live = false;
      clearTimeout(timer);
      for (const c of contents.values()) if (c.kind === "image") URL.revokeObjectURL(c.url);
    };
  });

  const index = $derived(top !== null && top.ok ? top.index : null);
  const shown = $derived(index === null ? null : head(index, fileName));

  let typed = $state("");
  let tab = $state<"contents" | "deps">("contents");
  const words = $derived(terms(typed));

  // Plain copies of what is open, read once a pass: the rows ask for each of thousands.
  const filtered = $derived.by(() => {
    if (top === null || !top.ok) return null;
    const open = { dirs: new Set(openDirs), files: new Set(openFiles) };
    const got = new Map(nested);
    if (tab === "deps") return { rows: libRows(top.index, words, open, got), hits: null };
    if (words.length > 0) return filteredRows(top.listing, words, open, got);
    return { rows: treeRows(top.listing, "", 0, open, got), hits: null };
  });
  const rows = $derived(filtered?.rows ?? []);
  const total = $derived(top !== null && top.ok ? top.listing.names.length : 0);
  const libs = $derived(index?.libs.length ?? 0);

  const win = new Windowed<string>({
    count: () => rows.length,
    key: (i) => rows[i]!.k,
    estimate: 29,
    estimateOf: (k) => (k.startsWith("d") ? 360 : 29),
  });

  /** A read on `scope`'s archive, opened again once if the backend let it go (it keeps the last eight). */
  async function onArchive<T>(scope: string, work: (handle: string) => Promise<T>): Promise<T> {
    const handle = await handleOf(scope);
    try {
      return await work(handle);
    } catch (error) {
      if (!gone(error)) throw error;
      handles.clear();
      return work(await handleOf(scope));
    }
  }

  function gone(error: unknown): boolean {
    return typeof error === "object" && error !== null && (error as { code?: string }).code === "not_found" && ipcErrorMessage(error).includes("no longer open");
  }

  async function handleOf(scope: string): Promise<string> {
    const known = handles.get(scope);
    if (known !== undefined) return known;
    if (scope === "") {
      const index = await ports.open();
      handles.set("", index.handle);
      return index.handle;
    }
    const cut = scope.lastIndexOf("!");
    const [parent, entry] = cut < 0 ? ["", scope] : [scope.slice(0, cut), scope.slice(cut + 1)];
    const index = await onArchive(parent, (h) => ports.nested(h, entry));
    handles.set(scope, index.handle);
    return index.handle;
  }

  function listingOf(scope: string): Listing | null {
    if (scope === "") return top !== null && top.ok ? top.listing : null;
    const got = nested.get(scope);
    return typeof got === "object" && got.ok ? got.listing : null;
  }

  function toggleDir(row: Extract<Row, { kind: "dir" }>) {
    const key = `${row.scope}|${row.dir.path}`;
    if (openDirs.has(key)) openDirs.delete(key);
    else openDirs.add(key);
  }

  /** Opens a nested archive below its entry once it is listed, so its tree draws whole. */
  async function openNested(scope: string, entry: string) {
    const key = `${scope}|${entry}`;
    const inner = scopeOf(scope, entry);
    if (!nested.has(inner)) {
      reading.add(key);
      try {
        const index = await onArchive(scope, (h) => ports.nested(h, entry));
        handles.set(inner, index.handle);
        const l = listing(index);
        if (l.tree.dirs.length === 1 && l.tree.files.length === 0) openDirs.add(`${inner}|${l.tree.dirs[0]!.path}`);
        nested.set(inner, { ok: true, listing: l });
      } catch (error) {
        nested.set(inner, { ok: false, error: ipcErrorMessage(error) });
      } finally {
        reading.delete(key);
      }
    }
    openFiles.add(key);
  }

  async function openEntry(scope: string, entry: string, flags: number) {
    const key = `${scope}|${entry}`;
    if (openFiles.has(key)) {
      openFiles.delete(key);
      return;
    }
    if (reading.has(key)) return;
    const kind = kindOf(entry);
    if (flags & UNSAFE) contents.set(key, { kind: "note", text: "Its name leaves the archive (an absolute path or `..`), so knobas does not open it." });
    else if (flags & UNREADABLE) contents.set(key, { kind: "note", text: "It is encrypted, or packed in a way knobas does not read." });
    else if (kind === "archive") return openNested(scope, entry);
    else if (!contents.has(key)) {
      reading.add(key);
      try {
        contents.set(key, await readContent(scope, entry, kind));
      } catch (error) {
        contents.set(key, { kind: "note", text: ipcErrorMessage(error) });
      } finally {
        reading.delete(key);
      }
    }
    openFiles.add(key);
  }

  async function readContent(scope: string, entry: string, kind: ReturnType<typeof kindOf>): Promise<Content> {
    if (kind === "class") return { kind: "class", outline: await onArchive(scope, (h) => ports.cls(h, entry)) };
    const got = await onArchive(scope, (h) => ports.entry(h, entry));
    if (kind === "image") return { kind: "image", url: URL.createObjectURL(typedImageBlob(got, imageType(entry))) };
    if (kind === "pdf") return { kind: "pdf", bytes: got };
    const text = textOf(got);
    if (text === null) return { kind: "binary", size: got.length };
    return kind === "markdown" ? { kind: "markdown", text } : { kind: "text", text };
  }

  async function setMode(scope: string, entry: string, mode: "outline" | "source") {
    const key = `${scope}|${entry}`;
    modes.set(key, mode);
    if (mode !== "source" || sources.has(key)) return;
    sources.set(key, "reading");
    try {
      const text = await onArchive(scope, (h) => ports.decompile(h, entry));
      sources.set(key, { text });
    } catch (error) {
      // A newer decompile stopped this one: asked again, it runs again.
      const message = ipcErrorMessage(error);
      if (message.includes("another class was decompiled")) sources.delete(key);
      else sources.set(key, { error: message });
    }
  }

  async function save(entry: string | null, scope = "") {
    saved = null;
    try {
      const where = await onArchive(scope, (h) => ports.download(h, entry));
      saved = { text: `Saved to ${where}`, failed: false };
    } catch (error) {
      saved = { text: ipcErrorMessage(error), failed: true };
    }
  }

  function pick(next: "contents" | "deps") {
    tab = next;
  }

  /** An entry's size, packed beside it where packing saved something. */
  function sizeTitle(l: Listing, i: number): string {
    const s = l.sizes[i] ?? 0;
    const p = l.packed[i] ?? 0;
    return p < s ? `${bytes(s)}, ${bytes(p)} packed` : `${bytes(s)}, stored`;
  }

  const ratio = $derived(index === null || index.total === 0 ? null : Math.round((1 - index.packed / index.total) * 100));
</script>

<div class="tf-jar">
  {#if top === null}
    <div class="tf-jar-wait">{#if slow}<p class="tf-dim" role="status">Reading {fileName}…</p>{/if}</div>
  {:else if !top.ok}
    <div class="tf-jar-bad" role="alert">
      <p><b>{fileName} could not be read as an archive.</b></p>
      <pre>{top.error}</pre>
    </div>
  {:else if index !== null && shown !== null}
    <header class="tf-jar-head">
      <h2 class="tf-jar-title">
        <span class="tf-jar-name">{shown.title}</span>
        {#if shown.version !== null}<span class="tf-jar-ver">{shown.version}</span>{/if}
      </h2>
      <p class="tf-jar-sub">
        <span>{shown.kind}</span>
        {#if shown.coords !== null}<code>{shown.coords}</code>{/if}
      </p>
      <dl class="tf-jar-facts">
        {#each shown.facts as f (f.label)}
          <dt>{f.label}</dt>
          <dd>{#if f.code}<code>{f.value}</code>{:else}{f.value}{/if}</dd>
        {/each}
        <dt>Size</dt>
        <dd>
          {bytes(index.size)}{#if index.prefix > 0}<span class="tf-dim">, a launch script of {bytes(index.prefix)} before it</span>{/if};
          {total.toLocaleString("en")} {total === 1 ? "entry" : "entries"}, {bytes(index.total)} unpacked{#if ratio !== null && ratio > 0}<span class="tf-dim">, packed {ratio} % smaller</span>{/if}
        </dd>
      </dl>
      <div class="tf-jar-folds">
        {#if index.manifest.length > 0}
          <details class="tf-jar-fold">
            <summary>Manifest <span class="tf-dim">{index.manifest.length} {index.manifest.length === 1 ? "field" : "fields"}</span></summary>
            <dl class="tf-jar-manifest">
              {#each index.manifest as [k, v], n (n)}<dt><code>{k}</code></dt><dd>{v}</dd>{/each}
            </dl>
          </details>
        {/if}
        {#if shown.classPath.length > 0}
          <details class="tf-jar-fold">
            <summary>Class path <span class="tf-dim">{shown.classPath.length}</span></summary>
            <ul class="tf-jar-cp">{#each shown.classPath as p, n (n)}<li><code>{p}</code></li>{/each}</ul>
          </details>
        {/if}
        {#if index.maven.length > 1}
          <details class="tf-jar-fold">
            <summary>Maven coordinates inside <span class="tf-dim">{index.maven.length - 1}</span></summary>
            <ul class="tf-jar-cp">
              {#each index.maven.slice(1) as m (m.path)}<li><code>{m.group}:{m.artifact}</code> <span class="tf-jar-vers">{m.version}</span></li>{/each}
            </ul>
          </details>
        {/if}
        {#if index.warnings.length > 0}
          <details class="tf-jar-fold tf-jar-warns" open>
            <summary>{index.warnings.length === 1 ? "One thing the reading passed over" : `${index.warnings.length} things the reading passed over`}</summary>
            <ul>{#each index.warnings as w, n (n)}<li>{w}</li>{/each}</ul>
          </details>
        {/if}
      </div>
      <p class="tf-jar-save">
        <button type="button" class="tf-btn tf-sm" onclick={() => void save(null)}>Save to Downloads</button>
        <span class="tf-dim tf-jar-saved" class:tf-jar-err={saved?.failed} role="status">{saved?.text ?? ""}</span>
      </p>
    </header>

    <div class="tf-jar-band">
      <label class="tf-minif">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input placeholder={tab === "deps" ? "Filter dependencies" : "Filter entries"} aria-label={tab === "deps" ? "Filter dependencies" : "Filter entries"} bind:value={typed} {@attach filterKeys()} />
      </label>
      <span class="tf-jar-count" aria-live="polite">
        {#if tab === "deps"}{words.length === 0 ? `${libs} ${libs === 1 ? "JAR" : "JARs"}` : `${rows.filter((r) => r.kind === "lib").length} of ${libs}`}
        {:else if words.length === 0}{total.toLocaleString("en")} {total === 1 ? "entry" : "entries"}
        {:else}{(filtered?.hits ?? 0).toLocaleString("en")} of {total.toLocaleString("en")}{/if}
      </span>
      {#if libs > 0}
        <span class="tf-trail">
          <span class="tf-seg" role="group" aria-label="Show">
            <button type="button" aria-pressed={tab === "contents"} onclick={() => pick("contents")}>Contents</button>
            <button type="button" aria-pressed={tab === "deps"} onclick={() => pick("deps")}>Dependencies</button>
          </span>
        </span>
      {/if}
    </div>

    {#if rows.length === 0}
      <p class="tf-jar-none">{words.length > 0 ? "Nothing in the archive matches the filter." : "The archive is empty."}</p>
    {/if}
    <div class="tf-jar-list" {@attach win.list}>
      {#each win.slice(rows) as row (row.k)}
        {@render drawRow(row)}
      {/each}
    </div>
  {/if}
</div>

{#snippet caret(open: boolean)}
  <svg class="tf-jar-caret" class:tf-jar-open={open} viewBox="0 0 10 10" aria-hidden="true"><path d="M3.5 2 7 5 3.5 8" /></svg>
{/snippet}

{#snippet drawRow(row: Row)}
  {#if row.kind === "dir"}
    <div class="tf-jar-row" data-k={row.k} data-depth={row.depth} {@attach win.row(row.k)} {@attach indent(row.depth)}>
      <button type="button" class="tf-jar-rbtn" aria-expanded={row.open} onclick={() => toggleDir(row)}>
        {@render caret(row.open)}
        <span class="tf-jar-dname">{row.label}/</span>
        <span class="tf-jar-meta">{row.dir.count.toLocaleString("en")} {row.dir.count === 1 ? "file" : "files"}</span>
        <span class="tf-jar-size">{bytes(row.dir.size)}</span>
      </button>
    </div>
  {:else if row.kind === "file"}
    {@const l = listingOf(row.scope)}
    {@const flags = l?.flags[row.i] ?? 0}
    {@const key = `${row.scope}|${row.path}`}
    {@const kind = kindOf(row.path)}
    <div class="tf-jar-row" data-k={row.k} {@attach win.row(row.k)} {@attach indent(row.depth)}>
      <button type="button" class="tf-jar-rbtn" class:tf-jar-bad-name={(flags & UNSAFE) !== 0} aria-expanded={row.open} onclick={() => void openEntry(row.scope, row.path, flags)}>
        {#if kind === "archive"}{@render caret(row.open)}{:else}<span class="tf-jar-nocaret"></span>{/if}
        <span class="tf-jar-fname" class:tf-jar-class={kind === "class"}>{row.label}</span>
        <span class="tf-jar-meta">
          {#if (flags & UNSAFE) !== 0}<span class="tf-jar-warn">Unsafe name</span>
          {:else if (flags & UNREADABLE) !== 0}<span class="tf-jar-warn">Encrypted</span>
          {:else if (flags & DUPLICATE) !== 0}<span class="tf-jar-warn">Listed twice</span>{/if}
        </span>
        <span class="tf-jar-size" title={l === null ? undefined : sizeTitle(l, row.i)}>{reading.has(key) ? "Reading…" : bytes(l?.sizes[row.i] ?? 0)}</span>
      </button>
    </div>
  {:else if row.kind === "lib"}
    {@const key = `|${row.path}`}
    <div class="tf-jar-row" data-k={row.k} {@attach win.row(row.k)} {@attach indent(row.depth)}>
      <button type="button" class="tf-jar-rbtn tf-jar-lib" aria-expanded={row.open} onclick={() => void openEntry("", row.path, 0)}>
        {@render caret(row.open)}
        <span class="tf-jar-fname">{row.name}</span>
        <span class="tf-jar-vers">{row.version ?? ""}</span>
        <span class="tf-jar-size">{reading.has(key) ? "Reading…" : bytes(row.size)}</span>
      </button>
    </div>
  {:else if row.kind === "note"}
    <div class="tf-jar-row tf-jar-rnote" data-k={row.k} {@attach win.row(row.k)} {@attach indent(row.depth)}><p>{row.text}</p></div>
  {:else if row.kind === "detail"}
    {@const key = `${row.scope}|${row.path}`}
    {@const c = contents.get(key)}
    <div class="tf-jar-detail" data-k={row.k} {@attach win.row(row.k)} {@attach indent(row.depth)}>
      {#if c === undefined}
        <!-- Read before it opens: nothing to draw here. -->
      {:else if c.kind === "class"}
        <TfJarClass
          outline={c.outline}
          path={row.path}
          {java}
          source={sources.get(key) ?? null}
          mode={modes.get(key) ?? "outline"}
          onmode={(m) => void setMode(row.scope, row.path, m)}
          {render}
        />
      {:else if c.kind === "text"}
        <TfJarCode text={c.text} path={row.path} {render} />
      {:else if c.kind === "markdown"}
        <div class="tf-jar-md"><TfMarkdown text={c.text} {render} /></div>
      {:else if c.kind === "image"}
        <div class="tf-jar-img"><img src={c.url} alt={row.path} /></div>
      {:else if c.kind === "pdf"}
        {@const pdf = c.bytes}
        <TfPdf read={() => Promise.resolve(pdf)} size={pdf.length} {render} />
      {:else if c.kind === "binary"}
        <p class="tf-jar-dnote">A binary file of {bytes(c.size)}, not drawn. <button type="button" class="tf-btn tf-sm" onclick={() => void save(row.path, row.scope)}>Save to Downloads</button></p>
      {:else}
        <p class="tf-jar-dnote">{c.text}</p>
      {/if}
    </div>
  {/if}
{/snippet}
