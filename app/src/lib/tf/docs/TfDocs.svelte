<!--
  Docs (#859, requirements §10): one screen for `#/docs`, `#/docs/space/<key>`
  and `#/page/<id>[/<section>]`, as prototype-6's `renderDocs` → `renderPage`.

  **Left**, the space picker (*All spaces (N)*, then every space with its page
  count; it follows the open page's space until you pick one) and the page
  tree (R-10.1, R-10.2). **Middle**, the page at most 74 characters wide in the
  reading face: the breadcrumb, the title, who edited it when, the version,
  the reading time and the labels, *Open in <source>*, the blocks and the
  comments (R-10.9 to R-10.11). `#/docs` opens the page read last, and every
  page opened is a visit (`visit_entity`), which is what "read last" reads.

  Drawn over the declared Docs frame (ADR-0022): `docs_tree` and `docs_page`.
  The search field, *For you*, the right rail, *Export as Markdown*, *Copy
  link*, the section edits, the composer, *New page* and the *This job* scope
  are #860 to #863's.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import {
    docsPage as realDocsPage,
    docsTree as realDocsTree,
    visitEntity as realVisitEntity,
    type DocPage,
    type DocsTree,
  } from "../../ipc/entity";
  import { openExternal as realOpenExternal } from "../../shell/open-external";
  import { push } from "../../shell/toasts.svelte";
  import { jobHash } from "../route";
  import { initials, when } from "../tickets/tickets";
  import TfDocBlocks from "./TfDocBlocks.svelte";
  import TfDocTree from "./TfDocTree.svelte";
  import {
    firstPage,
    pageHash,
    pageToOpen,
    person,
    placeOf,
    readingMinutes,
    spaceId,
    spaceLabel,
    spaceOfPage,
  } from "./docs";

  export interface DocsPorts {
    docsTree: typeof realDocsTree;
    docsPage: typeof realDocsPage;
    visitEntity: typeof realVisitEntity;
    openExternal: (url: string) => Promise<void>;
    copy: (text: string) => Promise<void>;
  }

  let {
    ready,
    rest,
    job,
    ports,
  }: {
    ready: boolean;
    /** What follows the address's first segment. */
    rest: string[];
    /** The job the reader was last in, for the breadcrumb's chip. */
    job: string | null;
    ports?: DocsPorts;
  } = $props();

  const io: DocsPorts = $derived(
    ports ?? {
      docsTree: realDocsTree,
      docsPage: realDocsPage,
      visitEntity: realVisitEntity,
      openExternal: realOpenExternal,
      copy: (text) => navigator.clipboard.writeText(text),
    },
  );

  let tree = $state<DocsTree | null>(null);
  let treeError = $state<string | null>(null);
  let page = $state<DocPage | null>(null);
  let pageError = $state<string | null>(null);
  /** The picked space (`spaceId`), `*` for all, or `null` to follow the page. */
  let picked = $state<string | null>(null);

  const place = $derived(placeOf(rest));
  const target = $derived(tree === null ? null : pageToOpen(place, tree));

  $effect(() => {
    if (!ready) return;
    void io.docsTree().then(
      (answer) => {
        tree = answer;
        treeError = null;
      },
      (cause: unknown) => (treeError = ipcErrorMessage(cause)),
    );
  });

  $effect(() => {
    const id = target;
    if (!ready || id === null) return;
    let live = true;
    void io.docsPage(id).then(
      (answer) => {
        if (!live) return;
        page = answer;
        pageError = null;
        void io.visitEntity(id).catch(() => {});
      },
      (cause: unknown) => {
        if (live) pageError = ipcErrorMessage(cause);
      },
    );
    return () => {
      live = false;
    };
  });

  // A section address scrolls its heading into view once the page is drawn.
  $effect(() => {
    if (page === null || place.kind !== "page" || place.section === null) return;
    const heading = document.getElementById(`s-${place.section}`);
    heading?.scrollIntoView?.();
  });

  const pageSpace = $derived(tree && page ? spaceOfPage(tree, page.entity_id) : undefined);
  const shownSpace = $derived.by(() => {
    if (tree === null || picked === "*") return undefined;
    if (picked !== null) return tree.spaces.find((s) => spaceId(s) === picked);
    return pageSpace ?? tree.spaces[0];
  });
  const pickerValue = $derived(picked === "*" ? "*" : shownSpace ? spaceId(shownSpace) : "*");
  const total = $derived(tree?.spaces.reduce((n, s) => n + s.pages.length, 0) ?? 0);
  const parent = $derived(page?.ancestors.at(-1));
  const current = $derived(page?.entity_id ?? target);

  function pick(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    picked = value;
    if (value === "*" || tree === null) return;
    const space = tree.spaces.find((s) => spaceId(s) === value);
    if (space && space !== pageSpace) {
      const first = firstPage(space);
      if (first) location.hash = pageHash(first.entity_id);
    }
  }

  async function open(url: string): Promise<void> {
    try {
      await io.openExternal(url);
    } catch (cause) {
      push({ text: `Could not open it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function copy(code: string): Promise<void> {
    try {
      await io.copy(code);
      push({ text: "Copied" });
    } catch (cause) {
      push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  function firstName(name: string): string {
    return name.split(/\s+/)[0] ?? name;
  }
</script>

<div class="tf-rd">
  <nav class="tf-rd-tree" aria-label="Docs">
    <div class="tf-dl-head">
      <select aria-label="Space" value={pickerValue} onchange={pick}>
        <option value="*">All spaces ({total})</option>
        {#each tree?.spaces ?? [] as space (spaceId(space))}
          <option value={spaceId(space)}>{spaceLabel(space)}</option>
        {/each}
      </select>
    </div>
    <div class="tf-dl-body">
      {#if treeError !== null}
        <p class="tf-dl-note">Could not read the page tree: {treeError}</p>
      {:else if tree !== null && tree.spaces.length === 0}
        <p class="tf-dl-note">No pages are synced yet. A source with pages, such as Confluence, fills this tree.</p>
      {:else if tree !== null && picked === "*"}
        {#each tree.spaces as space (spaceId(space))}
          <details open={current !== null && space.pages.some((p) => p.entity_id === current)}>
            <summary class="tf-spc">{space.key} {space.name ?? ""} <span class="tf-count">{space.pages.length}</span></summary>
            <TfDocTree {space} {current} />
          </details>
        {/each}
      {:else if shownSpace}
        <TfDocTree space={shownSpace} {current} />
      {/if}
    </div>
  </nav>

  <section class="tf-rd-main">
    {#if pageError !== null}
      <div class="tf-empty"><p>Could not open this page: {pageError}</p></div>
    {:else if page !== null}
      <article class="tf-rd-page">
        <div class="tf-rd-crumb">
          {#if job !== null}
            <a class="tf-jobchip tf-card" href={jobHash(job)}>{job}</a> ›
          {/if}
          <a href="#/docs">Docs</a>
          {#if page.space}
            › <a href={`#/docs/space/${encodeURIComponent(page.space.key)}`}>{page.space.name ?? page.space.key}</a>
          {/if}
          {#if parent}
            › {#if parent.entity_id}<a href={pageHash(parent.entity_id)}>{parent.title}</a>{:else}{parent.title}{/if}
          {/if}
        </div>
        <h1 class="tf-rd-title">{page.title}</h1>
        <div class="tf-rd-meta">
          {#if page.editor !== null || page.updated_at !== null}
            <span>{person(page.editor, page.editor_name, page.me)} edited {when(page.updated_at)}</span>
          {/if}
          {#if page.version !== null}<span>version {page.version}</span>{/if}
          {#if page.readable}<span>{readingMinutes(page)} min read</span>{/if}
          {#each page.labels as label (label)}<span class="tf-chip">{label}</span>{/each}
        </div>
        <div class="tf-rd-acts">
          {#if page.web_url}
            {@const url = page.web_url}
            <button type="button" class="tf-btn tf-sm tf-ghost tf-openin" onclick={() => open(url)}>
              Open in {page.source_name}
            </button>
          {/if}
        </div>
        <div class="tf-doc">
          {#if page.readable}
            <TfDocBlocks
              blocks={page.blocks}
              entityId={page.entity_id}
              sourceName={page.source_name}
              webUrl={page.web_url}
              onopen={open}
              oncopy={copy}
              anchors
            />
          {:else}
            <p class="tf-dl-note">knobas cannot read this page's body. Open it in {page.source_name}.</p>
          {/if}
          <div class="tf-cmts">
            <h2>Comments {#if page.comments.length > 0}<span class="tf-count">{page.comments.length}</span>{/if}</h2>
            {#each page.comments as comment, i (i)}
              {@const mine = comment.author !== null && page.me.includes(comment.author)}
              <div class="tf-msg">
                <span class="tf-av" class:tf-me={mine}>{initials(comment.author)}</span>
                <div class="tf-cmt-body">
                  <div class="tf-who">
                    <b>{mine ? "You" : firstName(person(comment.author, comment.author_name, page.me))}</b>
                    {comment.at === null ? "" : when(comment.at)}
                  </div>
                  <TfDocBlocks
                    blocks={comment.blocks}
                    entityId={page.entity_id}
                    sourceName={page.source_name}
                    webUrl={page.web_url}
                    onopen={open}
                    oncopy={copy}
                  />
                </div>
              </div>
            {:else}
              <p class="tf-dl-note">No comments yet.</p>
            {/each}
          </div>
        </div>
      </article>
    {:else if tree !== null && target === null}
      <div class="tf-empty"><p>Nothing to read yet. Pages appear here once a docs source has synced.</p></div>
    {/if}
  </section>
</div>
