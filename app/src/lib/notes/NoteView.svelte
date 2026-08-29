<!--
  A note, open in the slide-over — the first kind knobas owns rather than
  mirrors (#46).

  ## Why this is not `Detail.svelte`

  `Detail` is one view for every kind on purpose (§3a), and everything it draws
  is a *mirror row*: a source, a payload, a `web_url`, "synced 4 min ago". A
  note has none of those, so reusing it would mean five fields of placeholders
  a reader could not tell from real values — and none of the one thing a note
  needs, which is somewhere to type. It shares the slide-over's frame, its
  classes and its links panel; what it does not share is a body it cannot fill.

  ## Saving

  There is no *Save*. Story 2 is that no thought is lost to a closed window, so
  the row exists before the first keystroke (`create_note` takes no arguments)
  and every pause writes. Two things make that safe rather than merely
  convenient:

  * the write is **idempotent** — saving the same body twice reconciles to the
    same links and withdraws nothing — so a save that fires more often than it
    needs to costs a round trip and nothing else;
  * the answer carries the **refs it just reconciled**, which is what the chips
    below the editor redraw from. Fetching them separately would race the next
    keystroke.

  A save that fails says so and keeps what was typed: the text is in the
  textarea, and the next pause tries again.

  ## `[[` is the launcher

  Not "like" it: the same `Session` the launcher and *Link to…* drive, so the
  completion gets the same 90 ms debounce, the same request sequencing and the
  same selection model. Referring to a ticket is as fast as finding one because
  it *is* finding one.
-->
<script lang="ts">
  import { ipcErrorMessage, isIpcError } from "../ipc";
  import {
    deleteNote,
    getNote,
    saveNote,
    unlink,
    type LinkEntry,
    type NoteDetail,
  } from "../ipc/entity";
  import type { SearchHit } from "../ipc/search";
  import { launcherHome, search } from "../ipc/search";
  import LinkDialog from "../detail/LinkDialog.svelte";
  import LinksPanel from "../detail/LinksPanel.svelte";
  import { Session } from "../launcher/session.svelte";
  import Monogram from "../shell/Monogram.svelte";
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { ago } from "../shell/time";
  import { push } from "../shell/toasts.svelte";
  import NoteBody from "./NoteBody.svelte";
  import { activeRef, insertRef, type ActiveRef } from "./note-body";

  let {
    entityId,
    contextLabel,
    onclose,
    onnavigate,
    saveAfterMs = 700,
  }: {
    entityId: string;
    /** The room this was opened over, for the crumb. */
    contextLabel: string;
    onclose: () => void;
    /** Go to an address (spec §2). The shell owns navigation. */
    onnavigate: (hash: string) => void;
    /**
     * How long a pause counts as "stopped typing".
     *
     * A prop so a test can drive the autosave without waiting on wall-clock
     * time. Long enough that a sentence is one write rather than twenty; short
     * enough that closing the window a second after the last keystroke has
     * already saved.
     */
    saveAfterMs?: number | undefined;
  } = $props();

  let detail = $state<NoteDetail | null>(null);
  let error = $state<{ code: string; message: string } | null>(null);
  /** The generation of the newest read — see `Detail.svelte`. */
  let token = 0;

  /** What is in the fields, which may be ahead of what is in `detail`. */
  let title = $state("");
  let body = $state("");
  let editing = $state(false);
  let status = $state<"clean" | "typing" | "saving" | "saved" | "failed">("clean");
  let failure = $state<string | null>(null);
  /** Armed by the second click on *Delete*, because deleting a note is final. */
  let confirming = $state(false);
  let linking = $state(false);

  let timer: ReturnType<typeof setTimeout> | null = null;

  // svelte-ignore state_referenced_locally
  // Read once, like the launcher's and the link dialog's: a `Session` owns a
  // debounce timer and a request counter, and swapping its ports mid-life
  // would drop both.
  const session = new Session({ search, launcherHome });

  $effect(() => {
    const id = entityId;
    const mine = ++token;
    detail = null;
    error = null;
    void getNote(id)
      .then((answer) => {
        if (mine !== token) return;
        detail = answer;
        title = answer.note.title;
        body = answer.note.body_md;
      })
      .catch((rejection) => {
        if (mine !== token) return;
        error = {
          code: isIpcError(rejection) ? rejection.code : "internal",
          message: ipcErrorMessage(rejection),
        };
      });
  });

  /**
   * Nothing this panel armed may outlive it.
   *
   * Two timers, and both are real: the autosave's, and the `Session`'s own
   * keystroke debounce (`shell/residue.test.svelte.ts` is the pin, and it is
   * the class of bug this project has already shipped three times).
   *
   * The autosave timer is *dropped*, not flushed. A panel that wrote on its
   * way out would write the body the user had when they navigated away, over
   * whatever the note holds by the time the write lands -- and the last
   * keystroke is already covered, because leaving the field blurs it and a
   * blur flushes.
   */
  $effect(() => () => {
    if (timer !== null) clearTimeout(timer);
    session.dispose();
  });

  /**
   * Write after a pause.
   *
   * The timer is reset on every keystroke, so a sentence is one write. It is
   * *not* cancelled by {@link flush}: whoever flushes clears it first.
   */
  function schedule() {
    status = "typing";
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      void flush();
    }, saveAfterMs);
  }

  /**
   * Write now.
   *
   * The read generation is checked on the way back, exactly as the opening
   * read is: an answer for a note the address has already left must not
   * overwrite the panel that replaced it.
   */
  async function flush() {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    if (!detail) return;
    const mine = token;
    status = "saving";
    try {
      const answer = await saveNote(entityId, title, body);
      if (mine !== token) return;
      // The note's *own* fields are deliberately not written back: the answer
      // describes the body as it was sent, and the user may have typed since.
      // What is taken is what only the backend knows -- the refs it
      // reconciled, and the links as they now stand.
      detail = answer;
      status = "saved";
      failure = null;
    } catch (rejection) {
      if (mine !== token) return;
      status = "failed";
      failure = ipcErrorMessage(rejection);
    }
  }

  async function remove() {
    if (!confirming) {
      confirming = true;
      return;
    }
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    try {
      await deleteNote(entityId);
    } catch (rejection) {
      push({ text: `Could not delete this note: ${ipcErrorMessage(rejection)}`, tone: "err" });
      return;
    }
    onclose();
  }

  /** Re-read without blanking the panel, after a write somewhere else. */
  async function refresh() {
    const mine = token;
    try {
      const answer = await getNote(entityId);
      if (mine !== token) return;
      detail = answer;
    } catch (rejection) {
      push({ text: `Could not re-read this note: ${ipcErrorMessage(rejection)}`, tone: "err" });
    }
  }

  /**
   * Withdraw a link — unless the note's own body is what draws it.
   *
   * A `[[ref]]` link is *derived*: the body is the source of truth, so
   * unlinking one would be undone by the next save, silently. Saying where the
   * link comes from is the only honest answer the panel can give.
   */
  async function removeLink(entry: LinkEntry) {
    if (entry.link.origin === "implied" && entry.link.from_id === entityId) {
      push({
        text: "This link comes from a [[reference]] in the note. Remove the reference to withdraw it.",
        tone: "plain",
      });
      return;
    }
    try {
      await unlink(entry.link.id);
    } catch (rejection) {
      push({ text: `Could not unlink: ${ipcErrorMessage(rejection)}`, tone: "err" });
      return;
    }
    await refresh();
  }

  /* ---------------------------------------------------------------- `[[` */

  let editor = $state<HTMLTextAreaElement | null>(null);
  /** The `[[` the caret is in, or `null`. Recomputed on every input and move. */
  let active = $state<ActiveRef | null>(null);
  /** Set by `Escape`, cleared by the next keystroke: one rung, one press. */
  let dismissed = $state(false);

  /** Only `hit` rows are pickable — the picker is in results mode or empty. */
  const hits = $derived(session.rows.filter((row) => row.kind === "hit"));
  const picking = $derived(active !== null && !dismissed && hits.length > 0);

  function retarget(area: HTMLTextAreaElement) {
    active = activeRef(area.value, area.selectionStart);
    if (active === null) {
      session.set("");
      return;
    }
    session.type(active.query);
  }

  function ontype(event: Event & { currentTarget: HTMLTextAreaElement }) {
    body = event.currentTarget.value;
    dismissed = false;
    retarget(event.currentTarget);
    schedule();
  }

  /** Accept a completion: the body gains the reference, the caret follows it. */
  function complete(hit: SearchHit) {
    const area = editor;
    if (!area || !active) return;
    const written = insertRef(area.value, active, hit.entity_id);
    body = written.body;
    area.value = written.body;
    area.setSelectionRange(written.caret, written.caret);
    area.focus();
    active = null;
    session.set("");
    schedule();
  }

  function onkeydown(event: KeyboardEvent & { currentTarget: HTMLTextAreaElement }) {
    if (picking) {
      switch (event.key) {
        case "ArrowDown":
          event.preventDefault();
          session.move(1);
          return;
        case "ArrowUp":
          event.preventDefault();
          session.move(-1);
          return;
        case "Enter": {
          event.preventDefault();
          const row = session.current;
          if (row?.kind === "hit") complete(row.hit);
          return;
        }
        case "Escape":
          // Rung 1: dismiss the completion. The slide-over underneath stays.
          event.preventDefault();
          event.stopPropagation();
          dismissed = true;
          return;
        default:
      }
    }
    // Arrow keys and clicks move the caret without changing the text, so the
    // `[[` under it has to be recomputed after the browser has moved it.
    queueMicrotask(() => {
      if (editor) retarget(editor);
    });
  }

  /* ------------------------------------------------------------- chrome */

  let header = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const restoreTo = document.activeElement;
    header?.focus();
    return () => {
      if (restoreTo instanceof HTMLElement && document.contains(restoreTo)) {
        restoreTo.focus();
      }
    };
  });

  /** Unique per instance, so `aria-labelledby` and `for` cannot collide. */
  const ids = `note-${Math.random().toString(36).slice(2, 9)}`;

  const label = $derived(kindSingular("note", kindRegistry.info("note")));
  const monogram = $derived(kindMonogram("note", kindRegistry.info("note")));

  function labelOf(kind: string): string {
    return kindSingular(kind, kindRegistry.info(kind));
  }

  function monogramOf(kind: string): string {
    return kindMonogram(kind, kindRegistry.info(kind));
  }

  const saveWord = $derived(
    status === "saving"
      ? "Saving…"
      : status === "failed"
        ? "Not saved"
        : status === "typing"
          ? "Unsaved changes"
          : detail
            ? `Saved ${ago(detail.note.updated_at)}`
            : "",
  );
</script>

<aside class="detail" aria-labelledby="{ids}-title">
  <div class="d-h" bind:this={header} tabindex="-1">
    <span class="crumb">{contextLabel} <b>›</b> {label}</span>
    <span class="d-acts">
      {#if detail}
        <button class="btn sm" onclick={() => (editing = !editing)}>
          {editing ? "Done" : "Edit"}
        </button>
        <button class="btn sm" onclick={() => (linking = true)}>Link to…</button>
        <button class="btn sm" class:danger={confirming} onclick={() => void remove()}>
          {confirming ? "Really delete?" : "Delete"}
        </button>
      {/if}
    </span>
    <button class="x" aria-label="Close panel" title="Close (Esc)" onclick={onclose}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
    </button>
  </div>

  <div class="d-b">
    {#if error}
      <div class="d-title">
        <div>
          <h2 id="{ids}-title">
            {error.code === "not_found"
              ? "There is no note here"
              : error.code === "invalid"
                ? "That is not an entity address"
                : "Could not read this note"}
          </h2>
        </div>
      </div>
      <div class="sec">
        <p class="muted">{error.message}</p>
        <div class="acts"><button class="btn" onclick={onclose}>Close</button></div>
      </div>
    {:else if !detail}
      <div class="d-title">
        <div><h2 id="{ids}-title">Reading…</h2></div>
      </div>
    {:else}
      <div class="d-title">
        <div class="ttl">
          <span class="k"><Monogram text={monogram} label={label} /></span>
          <label class="vis-hidden" for="{ids}-name">Note title</label>
          <input
            class="name"
            id="{ids}-name"
            type="text"
            autocomplete="off"
            placeholder="Untitled note"
            bind:value={title}
            oninput={schedule}
            onblur={() => void flush()}
          />
        </div>
      </div>
      <h2 class="vis-hidden" id="{ids}-title">{title || "Untitled note"}</h2>

      <div class="sec">
        <div class="sec-h">
          <span class="lab">Note</span>
          <span class="k muted">{saveWord}</span>
        </div>

        {#if failure}
          <p class="fail">{failure} — what you typed is still here, and the next pause tries again.</p>
        {/if}

        {#if editing}
          <label class="vis-hidden" for="{ids}-body">Note body</label>
          <textarea
            class="inp editor"
            id="{ids}-body"
            bind:this={editor}
            value={body}
            placeholder="Markdown. Type [[ to refer to anything knobas knows."
            oninput={ontype}
            onkeydown={onkeydown}
            onclick={(event) => retarget(event.currentTarget)}
            onblur={() => void flush()}
          ></textarea>

          {#if picking}
            <div class="hits" role="listbox" aria-label="Reference targets">
              {#each session.rows as row, index (row.id)}
                {#if row.kind === "hit"}
                  <button
                    class="row g3 hit"
                    class:sel={index === session.selected}
                    role="option"
                    aria-selected={index === session.selected}
                    onmousedown={(event) => event.preventDefault()}
                    onclick={() => complete(row.hit)}
                    onmouseenter={() => (session.selected = index)}
                  >
                    <Monogram
                      text={monogramOf(row.hit.kind)}
                      label={labelOf(row.hit.kind)}
                    />
                    <!-- Raw source text, rendered as text (gotcha 7). -->
                    <span class="t">{row.hit.title}</span>
                    <span class="r">{labelOf(row.hit.kind)}</span>
                  </button>
                {/if}
              {/each}
            </div>
          {/if}
        {:else if body.trim() === ""}
          <p class="muted">Nothing written yet. <em>Edit</em> to start.</p>
        {:else}
          <NoteBody {body} refs={detail.refs} onopen={onnavigate} />
        {/if}
      </div>

      <LinksPanel
        entityId={entityId}
        links={detail.links}
        onopen={onnavigate}
        onunlink={(entry) => void removeLink(entry)}
        onlink={() => (linking = true)}
      />
    {/if}
  </div>
</aside>

{#if linking && detail}
  <LinkDialog
    fromId={entityId}
    fromTitle={title || "Untitled note"}
    onclose={() => (linking = false)}
    oncreated={() => {
      linking = false;
      void refresh();
    }}
  />
{/if}

<style>
  .ttl {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
  }

  .name {
    flex: 1;
    min-width: 0;
    padding: 2px 6px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    color: var(--text);
    font: 600 17px var(--disp);
  }

  .name:hover {
    border-color: var(--hair);
  }

  .name:focus-visible {
    border-color: var(--link);
    background: var(--raised);
  }

  .editor {
    display: block;
    width: 100%;
    min-height: 220px;
    padding: 8px;
    font: 13px/1.55 var(--mono);
    resize: vertical;
  }

  .danger {
    border-color: var(--fail);
    color: var(--fail);
  }

  .vis-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
