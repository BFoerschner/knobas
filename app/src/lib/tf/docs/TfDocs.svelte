<!--
  Docs (#859 to #862, requirements §10): one screen for `#/docs`,
  `#/docs/space/<key>` and `#/page/<id>[/<section>]`, as prototype-6's
  `renderDocs` → `renderPage`.

  **Left**, the space picker (*All spaces (N)*, then every space with its page
  count; it follows the open page's space until you pick one or start
  searching), the search field that names its scope (`/` focuses it), its
  one line of syntax, and under it *For you* and the page tree, or, with text
  in the field, the hits (R-10.1 to R-10.4, R-10.8). *New page* is at the
  bottom.

  **Middle**, the page at most 74 characters wide in the reading face: the
  breadcrumb, the title, who edited it when, the version, the reading time
  and the labels (each searches for itself), the actions, the blocks by
  section (*Edit section*) and the comments with the composer (R-10.9 to
  R-10.14). A page opened from a hit keeps the query: every match is marked
  and the find bar steps through them (R-10.5, R-10.12).

  **Right**, the rail (R-10.15). `#/docs` opens the page read last, and every
  page opened is a visit (`visit_entity`), which is what "read last" reads.

  Drawn over the declared Docs frame (ADR-0022): `docs_tree`, `docs_page`,
  `docs_search`; the writes go through the write queue. The *This job* scope
  is #863's.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import {
    createLink as realCreateLink,
    docsCreatePage as realDocsCreatePage,
    docsEditSection as realDocsEditSection,
    docsExportMarkdown as realDocsExportMarkdown,
    docsPage as realDocsPage,
    docsSearch as realDocsSearch,
    docsTree as realDocsTree,
    getEntity as realGetEntity,
    submitWrite as realSubmitWrite,
    visitEntity as realVisitEntity,
    type DocPage,
    type DocsHits,
    type DocsTree,
    type NewPage,
  } from "../../ipc/entity";
  import { listSources as realListSources } from "../../ipc/sources";
  import { drawn } from "../../detail/relations";
  import { openExternal as realOpenExternal } from "../../shell/open-external";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { resolveTicket } from "../job/job";
  import TfLinkTo from "../job/TfLinkTo.svelte";
  import type { JobPorts } from "../job/ports";
  import { jobHash } from "../route";
  import { initials, when } from "../tickets/tickets";
  import TfDocBlocks from "./TfDocBlocks.svelte";
  import TfDocBody from "./TfDocBody.svelte";
  import TfDocHits from "./TfDocHits.svelte";
  import TfDocRail from "./TfDocRail.svelte";
  import TfDocTree from "./TfDocTree.svelte";
  import TfNewPage from "./TfNewPage.svelte";
  import {
    copyLinkText,
    firstPage,
    forYouWhy,
    inlineWords,
    markText,
    pageHash,
    pageToOpen,
    person,
    placeOf,
    readingMinutes,
    scopeName,
    sectionsOf,
    spaceId,
    spaceLabel,
    spaceOfPage,
    stemsOf,
  } from "./docs";

  /** The page's own reads and writes; *Link to…* uses the job's ports. */
  export interface DocsPorts {
    docsTree: typeof realDocsTree;
    docsPage: typeof realDocsPage;
    docsSearch: typeof realDocsSearch;
    docsExportMarkdown: typeof realDocsExportMarkdown;
    docsEditSection: typeof realDocsEditSection;
    docsCreatePage: typeof realDocsCreatePage;
    submitWrite: typeof realSubmitWrite;
    visitEntity: typeof realVisitEntity;
    openExternal: (url: string) => Promise<void>;
    copy: (text: string) => Promise<void>;
  }

  type LinkPorts = Pick<JobPorts, "listSources" | "getEntity" | "createLink" | "search" | "launcherHome">;

  let {
    ready,
    rest,
    job,
    ports,
    linkPorts,
    layers = sharedLayers,
  }: {
    ready: boolean;
    /** What follows the address's first segment. */
    rest: string[];
    /** The job the reader was last in, for the breadcrumb's chip and *Link to <job>*. */
    job: string | null;
    ports?: DocsPorts;
    linkPorts?: LinkPorts;
    layers?: Layers;
  } = $props();

  const io: DocsPorts = $derived(
    ports ?? {
      docsTree: realDocsTree,
      docsPage: realDocsPage,
      docsSearch: realDocsSearch,
      docsExportMarkdown: realDocsExportMarkdown,
      docsEditSection: realDocsEditSection,
      docsCreatePage: realDocsCreatePage,
      submitWrite: realSubmitWrite,
      visitEntity: realVisitEntity,
      openExternal: realOpenExternal,
      copy: (text) => navigator.clipboard.writeText(text),
    },
  );
  const links: LinkPorts = $derived(
    linkPorts ?? {
      listSources: () => realListSources(),
      getEntity: (id) => realGetEntity(id),
      createLink: (from, to, relation, note) => realCreateLink(from, to, relation, note),
      search: async () => {
        throw new Error("search is not wired here");
      },
      launcherHome: async () => {
        throw new Error("search is not wired here");
      },
    },
  );

  let tree = $state<DocsTree | null>(null);
  let treeError = $state<string | null>(null);
  let page = $state<DocPage | null>(null);
  let pageError = $state<string | null>(null);
  /** The picked space (`spaceId`), `*` for all, or `null` to follow the page. */
  let picked = $state<string | null>(null);

  /** The search field's text (#860). */
  let query = $state("");
  let found = $state<DocsHits | null>(null);
  let searchError = $state<string | null>(null);
  /** What the find bar marks: the needles a hit was opened with. */
  let find = $state("");
  let field = $state<HTMLInputElement | null>(null);

  const place = $derived(placeOf(rest));
  const target = $derived(tree === null ? null : pageToOpen(place, tree));

  async function readTree() {
    try {
      tree = await io.docsTree();
      treeError = null;
    } catch (cause) {
      treeError = ipcErrorMessage(cause);
    }
  }

  $effect(() => {
    if (!ready) return;
    untrack(() => void readTree());
  });

  $effect(() => {
    const id = target;
    if (!ready || id === null) return;
    pageError = null;
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
  const searching = $derived(query.trim() !== "");
  const forYou = $derived(
    (tree?.for_you ?? []).filter((f) => shownSpace === undefined || f.space_key === shownSpace.key),
  );

  // Every keystroke searches; an answer to an older keystroke is dropped.
  let asked = 0;
  $effect(() => {
    const text = query;
    const scope = shownSpace ? { source_id: shownSpace.source_id, key: shownSpace.key } : null;
    if (!ready || text.trim() === "") {
      found = null;
      searchError = null;
      return;
    }
    const mine = ++asked;
    void io.docsSearch(text, scope).then(
      (answer) => {
        if (mine !== asked) return;
        found = answer;
        searchError = null;
      },
      (cause: unknown) => {
        if (mine === asked) searchError = ipcErrorMessage(cause);
      },
    );
  });

  function pick(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    picked = value;
    if (value === "*" || tree === null || searching) return;
    const space = tree.spaces.find((s) => spaceId(s) === value);
    if (space && space !== pageSpace) {
      const first = firstPage(space);
      if (first) location.hash = pageHash(first.entity_id);
    }
  }

  function typed(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    // Typing keeps the search in the space the picker shows, so opening a hit
    // in another space does not narrow the hits under the reader.
    if (picked === null && value.trim() !== "" && shownSpace) picked = spaceId(shownSpace);
    query = value;
    // Clearing the field brings the tree back and drops the highlighting.
    if (value.trim() === "") find = "";
  }

  /** Open a hit with its query carried over. */
  function openHit(entityId: string): void {
    find = found?.needles.join(" ") ?? "";
    location.hash = pageHash(entityId);
  }

  function onsearchkey(event: KeyboardEvent): void {
    const hits = found?.hits ?? [];
    if (!searching || hits.length === 0) return;
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp" && event.key !== "Enter") return;
    event.preventDefault();
    const at = hits.findIndex((h) => h.entity_id === current);
    const to =
      event.key === "Enter"
        ? Math.max(at, 0)
        : Math.max(0, Math.min(hits.length - 1, at + (event.key === "ArrowDown" ? 1 : -1)));
    openHit(hits[to]!.entity_id);
  }

  function searchLabel(label: string): void {
    query = label;
    find = "";
    field?.focus();
  }

  // `/` focuses the search from anywhere in Docs.
  $effect(() => {
    const onkey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
      const at = document.activeElement;
      if (at instanceof HTMLInputElement || at instanceof HTMLTextAreaElement || at instanceof HTMLSelectElement) return;
      if (at instanceof HTMLElement && at.isContentEditable) return;
      event.preventDefault();
      field?.focus();
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });

  // ---- the find bar (R-10.12) ----
  const stems = $derived(stemsOf(find));
  let article = $state<HTMLElement | null>(null);
  let main = $state<HTMLElement | null>(null);
  let marks = $state(0);
  let at = $state(-1);

  function jump(step: number): void {
    const all = article ? [...article.querySelectorAll("mark")] : [];
    marks = all.length;
    if (all.length === 0) return;
    at = (at + step + all.length) % all.length;
    all.forEach((m, i) => m.classList.toggle("tf-cur", i === at));
    all[at]?.scrollIntoView?.({ block: "center" });
  }

  // A page drawn with a query marks its first match, unless the address names
  // a section, which is scrolled to instead.
  $effect(() => {
    const p = page;
    void stems;
    if (p === null) return;
    void tick().then(() => {
      at = -1;
      if (place.kind === "page" && place.section !== null) {
        document.getElementById(`s-${place.section}`)?.scrollIntoView?.();
        marks = article ? article.querySelectorAll("mark").length : 0;
      } else if (stems.length > 0) jump(1);
      else marks = 0;
      follow();
    });
  });

  function onfindkey(event: KeyboardEvent): void {
    if (event.key !== "Enter") return;
    event.preventDefault();
    jump(event.shiftKey ? -1 : 1);
  }

  // ---- the outline follows the scrolling ----
  let active = $state<number | null>(null);
  function follow(): void {
    if (!main || !article) return;
    const top = main.getBoundingClientRect().top + 90;
    let now: number | null = null;
    for (const h of article.querySelectorAll<HTMLElement>("h2[id^='s-']")) {
      if (h.getBoundingClientRect().top < top) now = Number(h.id.slice(2));
    }
    active = now;
  }

  // ---- actions ----
  let linking = $state(false);
  let newPage = $state<{ space: string | null; parent: string | null } | null>(null);
  let comment = $state("");
  let commenting = $state(false);
  let composer = $state<HTMLTextAreaElement | null>(null);

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

  async function copyLink(p: DocPage): Promise<void> {
    try {
      await io.copy(copyLinkText(p));
      push({ text: p.web_url ? `Copied the knobas link and the ${p.source_name} link` : "Copied the knobas link" });
    } catch (cause) {
      push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function exportMarkdown(p: DocPage): Promise<void> {
    try {
      const done = await io.docsExportMarkdown(p.entity_id);
      push({ text: `Saved ${done.file} to ~/Downloads` });
    } catch (cause) {
      push({ text: `Could not export it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** The job's ticket, as `create_link` names it. */
  async function jobTicket(key: string): Promise<string | null> {
    const sources = await links.listSources().catch(() => []);
    const found = await resolveTicket(key, sources, links.getEntity);
    return found?.row.entity_id ?? null;
  }

  async function linkToJob(p: DocPage, key: string): Promise<void> {
    try {
      const ticket = await jobTicket(key);
      if (ticket === null) {
        push({ text: `${key} is not in the mirror`, tone: "err" });
        return;
      }
      const row = drawn("documents", p.entity_id, ticket);
      await links.createLink(row.fromId, row.toId, row.relation);
      push({ text: `Linked to ${key}` });
      page = await io.docsPage(p.entity_id);
    } catch (cause) {
      push({ text: `Could not link it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function saveSection(p: DocPage, section: number, text: string): Promise<boolean> {
    try {
      await io.docsEditSection(p.entity_id, section, p.version, text);
      const heading = sectionsOf(p.blocks)[section]?.heading;
      const name = heading ? inlineWords(heading.text) : "";
      push({ text: `Queued: edit section “${name}” of ${p.title}.` });
      return true;
    } catch (cause) {
      push({ text: `Could not queue the edit: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    }
  }

  async function sendComment(p: DocPage): Promise<void> {
    if (comment.trim() === "" || commenting) return;
    commenting = true;
    try {
      await io.submitWrite({ Comment: { entity: p.entity_id, body: comment } });
      push({ text: `Queued: comment on ${p.title}.` });
      comment = "";
    } catch (cause) {
      push({ text: `Could not queue the comment: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      commenting = false;
    }
  }

  async function createPage(next: NewPage, linkToJob: boolean): Promise<boolean> {
    try {
      const ticket = linkToJob && job !== null ? await jobTicket(job) : null;
      await io.docsCreatePage(next, ticket);
      push({ text: `Queued: create page ${next.title.trim()}.` });
      return true;
    } catch (cause) {
      push({ text: `Could not queue the page: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
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
      <label class="tf-fsearch">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
          ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
        >
        <input
          bind:this={field}
          value={query}
          oninput={typed}
          onkeydown={onsearchkey}
          data-filter
          placeholder="Search {scopeName(shownSpace)}"
          aria-label="Search pages"
          autocomplete="off"
        />
        <kbd>/</kbd>
      </label>
      <div class="tf-dl-hint"><code>"…"</code> phrase, <code>-word</code> leave out, <code>by:</code> <code>updated:7d</code></div>
    </div>
    <div class="tf-dl-body">
      {#if treeError !== null}
        <p class="tf-dl-note">Could not read the page tree: {treeError}</p>
      {:else if searching}
        {#if searchError !== null}
          <p class="tf-dl-note">Could not search: {searchError}</p>
        {:else if found !== null}
          <TfDocHits
            {found}
            {current}
            scope={scopeName(shownSpace)}
            inSpace={shownSpace !== undefined}
            me={page?.me ?? []}
            onall={() => (picked = "*")}
            onpick={openHit}
          />
        {/if}
      {:else}
        {#if forYou.length > 0}
          <div class="tf-dl-for">
            <h4>For you</h4>
            {#each forYou as item (item.entity_id)}
              <a href={pageHash(item.entity_id)} aria-current={item.entity_id === current ? "true" : undefined}
                >{item.title}<span class="tf-why">{forYouWhy(item)}</span></a
              >
            {/each}
          </div>
        {/if}
        {#if tree !== null && tree.spaces.length === 0}
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
      {/if}
    </div>
    {#if page?.writes.includes("create_page")}
      {@const p = page}
      <div class="tf-dl-foot">
        <button
          type="button"
          class="tf-btn tf-sm"
          onclick={() => {
            const space = shownSpace ?? pageSpace;
            newPage = { space: space ? spaceId(space) : null, parent: p.entity_id };
          }}
          >New page</button>
      </div>
    {/if}
  </nav>

  <section class="tf-rd-main" bind:this={main} onscroll={follow}>
    {#if find !== "" && page !== null}
      <div class="tf-findbar" role="search">
        <span>Matches for</span>
        <input value={find} oninput={(e) => (find = e.currentTarget.value)} onkeydown={onfindkey} aria-label="Find in page" />
        <span>{marks === 0 ? "none on this page" : `${Math.max(at, 0) + 1} of ${marks}`}</span>
        <button type="button" onclick={() => jump(-1)} aria-label="Previous">↑</button>
        <button type="button" onclick={() => jump(1)} aria-label="Next">↓</button>
        <button type="button" class="tf-findx" onclick={() => (find = "")} aria-label="Stop highlighting">✕</button>
      </div>
    {/if}
    {#if pageError !== null}
      <div class="tf-empty"><p>Could not open this page: {pageError}</p></div>
    {:else if page !== null}
      {@const p = page}
      <article class="tf-rd-page" bind:this={article}>
        <div class="tf-rd-crumb">
          {#if job !== null}
            <a class="tf-jobchip tf-card" href={jobHash(job)}>{job}</a> ›
          {/if}
          <a href="#/docs">Docs</a>
          {#if p.space}
            › <a href={`#/docs/space/${encodeURIComponent(p.space.key)}`}>{p.space.name ?? p.space.key}</a>
          {/if}
          {#if parent}
            › {#if parent.entity_id}<a href={pageHash(parent.entity_id)}>{parent.title}</a>{:else}{parent.title}{/if}
          {/if}
        </div>
        <h1 class="tf-rd-title">
          {#each markText(p.title, stems) as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
        </h1>
        <div class="tf-rd-meta">
          {#if p.editor !== null || p.updated_at !== null}
            <span>{person(p.editor, p.editor_name, p.me)} edited {when(p.updated_at)}</span>
          {/if}
          {#if p.version !== null}<span>version {p.version}</span>{/if}
          {#if p.readable}<span>{readingMinutes(p)} min read</span>{/if}
          {#each p.labels as label (label)}
            <button type="button" class="tf-chip tf-label" title="Search for {label}" onclick={() => searchLabel(label)}
              >{label}</button>
          {/each}
        </div>
        <div class="tf-rd-acts">
          {#if p.writes.includes("comment")}
            <button type="button" class="tf-btn tf-sm" onclick={() => composer?.focus()}>Comment</button>
          {/if}
          {#if job !== null}
            {@const key = job}
            <button type="button" class="tf-btn tf-sm" onclick={() => linkToJob(p, key)}>Link to {key}</button>
          {:else if linkPorts}
            <button type="button" class="tf-btn tf-sm" onclick={() => (linking = true)}>Link to…</button>
          {/if}
          {#if p.writes.includes("create_page")}
            <button
              type="button"
              class="tf-btn tf-sm"
              onclick={() => (newPage = { space: pageSpace ? spaceId(pageSpace) : null, parent: p.entity_id })}
              >New page here</button>
          {/if}
          {#if p.readable}
            <button type="button" class="tf-btn tf-sm" onclick={() => exportMarkdown(p)}>Export as Markdown</button>
          {/if}
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => copyLink(p)}>Copy link</button>
          {#if p.web_url}
            {@const url = p.web_url}
            <button type="button" class="tf-btn tf-sm tf-ghost tf-openin" onclick={() => open(url)}>
              Open in {p.source_name}
            </button>
          {/if}
        </div>
        <div class="tf-doc">
          {#if p.readable}
            <TfDocBody
              blocks={p.blocks}
              entityId={p.entity_id}
              sourceName={p.source_name}
              webUrl={p.web_url}
              sections={p.sections}
              editable={p.writes.includes("update_page") && p.version !== null}
              {stems}
              onopen={open}
              oncopy={copy}
              onsave={(section, text) => saveSection(p, section, text)}
            />
          {:else}
            <p class="tf-dl-note">knobas cannot read this page's body. Open it in {p.source_name}.</p>
          {/if}
          <div class="tf-cmts">
            <h2>Comments {#if p.comments.length > 0}<span class="tf-count">{p.comments.length}</span>{/if}</h2>
            {#each p.comments as c, i (i)}
              {@const mine = c.author !== null && p.me.includes(c.author)}
              <div class="tf-msg">
                <span class="tf-av" class:tf-me={mine}>{initials(c.author)}</span>
                <div class="tf-cmt-body">
                  <div class="tf-who">
                    <b>{mine ? "You" : firstName(person(c.author, c.author_name, p.me))}</b>
                    {c.at === null ? "" : when(c.at)}
                  </div>
                  <TfDocBlocks
                    blocks={c.blocks}
                    entityId={p.entity_id}
                    sourceName={p.source_name}
                    webUrl={p.web_url}
                    onopen={open}
                    oncopy={copy}
                    {stems}
                  />
                </div>
              </div>
            {:else}
              <p class="tf-dl-note">No comments yet.</p>
            {/each}
            {#if p.writes.includes("comment")}
              <div class="tf-compose">
                <textarea bind:this={composer} bind:value={comment} placeholder="Comment on this page" aria-label="Comment on this page"
                ></textarea>
                <div class="tf-acts">
                  <button
                    type="button"
                    class="tf-btn tf-primary"
                    disabled={commenting || comment.trim() === ""}
                    onclick={() => sendComment(p)}>Comment</button>
                </div>
              </div>
            {/if}
          </div>
        </div>
      </article>
    {:else if tree !== null && target === null}
      <div class="tf-empty"><p>Nothing to read yet. Pages appear here once a docs source has synced.</p></div>
    {/if}
  </section>

  {#if page !== null && pageError === null}
    <TfDocRail {page} {stems} {active} />
  {/if}
</div>

{#if linking && page !== null && linkPorts}
  {@const p = page}
  <TfLinkTo
    jobKey={p.title}
    entityId={p.entity_id}
    ports={links}
    {layers}
    onclose={() => (linking = false)}
    onlinked={() => {
      linking = false;
      push({ text: "Linked in knobas" });
      void io.docsPage(p.entity_id).then((answer) => (page = answer));
    }}
  />
{/if}

{#if newPage !== null && tree !== null}
  <TfNewPage
    {tree}
    space={newPage.space}
    parent={newPage.parent}
    {job}
    sourceName={page?.source_name ?? "the source"}
    {layers}
    onclose={() => (newPage = null)}
    oncreate={createPage}
  />
{/if}
