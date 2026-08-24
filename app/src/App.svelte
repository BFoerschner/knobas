<script lang="ts">
  import {
    demoLoad,
    ipcErrorMessage,
    recentActivity,
    search,
    type ActivityRow,
    type SearchHit,
    type SyncReport,
  } from "./lib/ipc";

  /** Long enough to swallow a burst of typing, short enough to feel live. */
  const DEBOUNCE_MS = 150;
  /** Result cap per query, per the M0 IPC surface. */
  const SEARCH_LIMIT = 30;
  const ACTIVITY_LIMIT = 20;

  let query = $state("");
  let hits = $state<SearchHit[]>([]);
  let searching = $state(false);
  let searchError = $state<string | null>(null);

  let report = $state<SyncReport | null>(null);
  let loadingDemo = $state(false);
  let demoError = $state<string | null>(null);

  let activity = $state<ActivityRow[]>([]);

  /**
   * Results arrive out of order — a slow query for `sep` can land after the
   * fast one for `sepa retry`. Only the newest issued query may write `hits`.
   */
  let issued = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => () => clearTimeout(timer));

  const groups = $derived(groupByKind(hits));

  /** Results in `kind` buckets, each bucket keeping the server's rank order. */
  function groupByKind(rows: SearchHit[]): Array<{ kind: string; hits: SearchHit[] }> {
    const byKind = new Map<string, SearchHit[]>();
    for (const hit of rows) {
      const bucket = byKind.get(hit.kind);
      if (bucket) {
        bucket.push(hit);
      } else {
        byKind.set(hit.kind, [hit]);
      }
    }
    return [...byKind].map(([kind, kindHits]) => ({ kind, hits: kindHits }));
  }

  function onInput(event: Event) {
    query = (event.currentTarget as HTMLInputElement).value;
    clearTimeout(timer);
    timer = setTimeout(() => void runSearch(query), DEBOUNCE_MS);
  }

  async function runSearch(text: string) {
    const trimmed = text.trim();
    const token = ++issued;
    if (trimmed === "") {
      hits = [];
      searching = false;
      searchError = null;
      return;
    }
    searching = true;
    try {
      const found = await search(trimmed, SEARCH_LIMIT);
      if (token !== issued) return;
      hits = found;
      searchError = null;
    } catch (error) {
      if (token !== issued) return;
      hits = [];
      searchError = ipcErrorMessage(error);
    } finally {
      if (token === issued) searching = false;
    }
  }

  async function onLoadDemo() {
    loadingDemo = true;
    demoError = null;
    try {
      report = await demoLoad();
      await refreshActivity();
      // The corpus just changed under whatever is on screen.
      await runSearch(query);
    } catch (error) {
      demoError = ipcErrorMessage(error);
    } finally {
      loadingDemo = false;
    }
  }

  async function refreshActivity() {
    try {
      activity = await recentActivity(ACTIVITY_LIMIT);
    } catch {
      // The activity strip is ambient; a failure here must not displace the
      // error from whatever the user actually asked for.
      activity = [];
    }
  }

  /** `2026-08-22T14:32:00Z` -> `2026-08-22 14:32`, in local time. */
  function formatTime(iso: string): string {
    const at = new Date(iso);
    if (Number.isNaN(at.getTime())) return iso;
    return at.toLocaleString(undefined, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
</script>

<header class="titlebar">
  <h1>knobas</h1>
  <span class="subtitle">M0 foundation</span>
  <button onclick={() => void onLoadDemo()} disabled={loadingDemo}>
    {loadingDemo ? "Loading…" : "Load demo data"}
  </button>
</header>

<main>
  {#if demoError}
    <p class="error">Demo load failed: {demoError}</p>
  {:else if report}
    <p class="report">
      Synced <strong>{report.upserted}</strong> items from
      <code>{report.source_id}</code>
      ({report.deleted} deleted, cursor <code>{report.cursor}</code>).
    </p>
  {/if}

  <label class="search">
    <span class="visually-hidden">Search</span>
    <input
      type="search"
      placeholder="Search synced work — try “sepa retry”"
      value={query}
      oninput={onInput}
      autocomplete="off"
      spellcheck="false"
    />
  </label>

  {#if searchError}
    <p class="error">Search failed: {searchError}</p>
  {:else if query.trim() === ""}
    <p class="hint">Load the demo data, then type to search it.</p>
  {:else if hits.length === 0}
    <p class="hint">{searching ? "Searching…" : "No matches."}</p>
  {:else}
    {#each groups as group (group.kind)}
      <section class="group">
        <h2>{group.kind} <span class="count">{group.hits.length}</span></h2>
        <ul>
          {#each group.hits as hit (hit.entity_id)}
            <li>
              <div class="row">
                <span class="title">{hit.title}</span>
                <span class="id">{hit.entity_id}</span>
              </div>
              <!--
                `snippet` is ts_headline output: `<b>` marks around unescaped
                source text. Interpolating it as text is the whole defence —
                never `{@html hit.snippet}`.
              -->
              <p class="snippet">{hit.snippet}</p>
              <div class="meta">
                <span>{hit.source_id}</span>
                <span>synced {formatTime(hit.synced_at)}</span>
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  {/if}

  {#if activity.length > 0}
    <section class="activity">
      <h2>Recent activity</h2>
      <ul>
        {#each activity as line (line.id)}
          <li>
            <span class="when">{formatTime(line.at)}</span>
            <span class="actor">{line.actor}</span>
            <span class="verb">{line.verb}</span>
            <span class="id">{line.entity_id ?? ""}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</main>

<style>
  /*
    Deliberately plain: M0 proves the pipe end to end. The mockup styling is
    M1 stream D, and porting it now would only make that port a rewrite.
  */
  :global(body) {
    margin: 0;
    font:
      14px/1.5 system-ui,
      sans-serif;
    color: #1c1f24;
    background: #fbfbfc;
  }

  .titlebar {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
    padding: 0.75rem 1.25rem;
    border-bottom: 1px solid #e3e5e9;
    background: #fff;
  }

  .titlebar h1 {
    margin: 0;
    font-size: 1.1rem;
    letter-spacing: 0.02em;
  }

  .subtitle {
    color: #7a808a;
    font-size: 0.8rem;
    flex: 1;
  }

  button {
    font: inherit;
    padding: 0.35rem 0.8rem;
    border: 1px solid #c9ccd3;
    border-radius: 5px;
    background: #f4f5f7;
    cursor: pointer;
  }

  button:disabled {
    cursor: progress;
    opacity: 0.6;
  }

  main {
    padding: 1.25rem;
    max-width: 52rem;
  }

  .search input {
    font: inherit;
    width: 100%;
    box-sizing: border-box;
    padding: 0.5rem 0.7rem;
    border: 1px solid #c9ccd3;
    border-radius: 5px;
    background: #fff;
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  .report,
  .hint,
  .error {
    margin: 0 0 0.9rem;
  }

  .hint {
    color: #7a808a;
  }

  .error {
    color: #a11b1b;
  }

  code {
    font-size: 0.85em;
    background: #eef0f3;
    padding: 0.05rem 0.3rem;
    border-radius: 3px;
  }

  .group,
  .activity {
    margin-top: 1.5rem;
  }

  .group h2,
  .activity h2 {
    margin: 0 0 0.5rem;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #7a808a;
  }

  .count {
    color: #a4a9b2;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .group li {
    padding: 0.6rem 0.75rem;
    border: 1px solid #e3e5e9;
    border-radius: 6px;
    background: #fff;
    margin-bottom: 0.4rem;
  }

  .row {
    display: flex;
    gap: 0.75rem;
    align-items: baseline;
  }

  .title {
    font-weight: 600;
  }

  .id,
  .meta,
  .when,
  .actor {
    color: #7a808a;
    font-size: 0.8rem;
  }

  .snippet {
    margin: 0.25rem 0;
    color: #40454d;
  }

  .meta {
    display: flex;
    gap: 0.9rem;
  }

  .activity li {
    display: flex;
    gap: 0.6rem;
    padding: 0.2rem 0;
  }

  .activity .verb {
    font-weight: 600;
  }
</style>
