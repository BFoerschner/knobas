<!--
  The detail slide-over — the mockup's `dShell` (`signal-miller.html:2546`).

  **One view for every kind, on purpose.** Spec §3a asks that an unknown kind
  get a generic detail view so a new ticket system is browsable on day one; in
  M1 that view is the only one there is. The tailored per-kind views of spec §5
  are write-backs, and even their read halves would need the per-adapter field
  table §3a forbids. What differs per kind here is the *header*: the adapter's
  own label and monogram when it declares them, the title-cased kind otherwise.
-->
<script lang="ts">
  import { ipcErrorMessage, isIpcError } from "../ipc";
  import { getEntity, type EntityDetail } from "../ipc/entity";
  import Monogram from "../shell/Monogram.svelte";
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { openExternal } from "../shell/open-external";
  import { ago } from "../shell/time";
  import { push } from "../shell/toasts.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
  import LinksPanel from "./LinksPanel.svelte";
  import PayloadView from "./PayloadView.svelte";
  import { projectPayload } from "./payload";

  let {
    entityId,
    kind,
    contextLabel,
    onclose,
  }: {
    entityId: string;
    /** From the address. `null` for the `#/entity/<id>` alias. */
    kind: string | null;
    /** The room this was opened over, for the crumb. */
    contextLabel: string;
    onclose: () => void;
  } = $props();

  let detail = $state<EntityDetail | null>(null);
  let error = $state<{ code: string; message: string } | null>(null);
  /** The generation of the newest request — see `Tile.svelte`. */
  let token = 0;

  $effect(() => {
    const id = entityId;
    const mine = ++token;
    detail = null;
    error = null;
    void getEntity(id)
      .then((answer) => {
        if (mine !== token) return;
        detail = answer;
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
   * Focus moves into the panel on open and back to the opener on close.
   *
   * Captured before the move, restored on teardown, and only if the opener is
   * still in the document — the row that opened a detail can be gone by the
   * time it closes, if a sync landed while it was open.
   */
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

  /** Unique per instance, so `aria-labelledby` cannot collide. */
  const titleId = `detail-title-${Math.random().toString(36).slice(2, 9)}`;

  /** The address's kind until the read lands, then the row's own. */
  const shownKind = $derived(detail?.row.kind ?? kind ?? "entity");
  /**
   * The adapter's word for this kind.
   *
   * `EntityDetail.kind_info` is the authoritative answer — the backend
   * resolves it by the source's *adapter* kind, so a `ticket` from two
   * adapters gets the right one of the two words. The registry is the fallback
   * for the frame drawn before `get_entity` answers, where the address already
   * carries a kind and the header would otherwise flash the humanised form.
   */
  const declared = $derived(detail?.kind_info ?? kindRegistry.info(shownKind));

  const label = $derived(kindSingular(shownKind, declared));
  const key = $derived(entityId.slice(entityId.indexOf(":") + 1));
  const fields = $derived(detail ? projectPayload(detail.payload) : []);
  /** Narrowed once, so the button and its handler agree that it is a string. */
  const webUrl = $derived(detail?.web_url ?? null);

  /**
   * Hand the item's own URL to the OS browser.
   *
   * `web_url` is a value an adapter mirrored out of a remote system, so
   * `openExternal` refuses anything that is not `http:`/`https:` — and its
   * refusal is shown, not swallowed: a source configured with an `ftp://`
   * base URL is something a person can fix.
   */
  async function open(url: string) {
    try {
      await openExternal(url);
    } catch (rejection) {
      push({ text: `Could not open the link: ${ipcErrorMessage(rejection)}`, tone: "err" });
    }
  }
</script>

<aside class="detail" aria-labelledby={titleId}>
  <div class="d-h" bind:this={header} tabindex="-1">
    <span class="crumb">{contextLabel} <b>›</b> {label}</span>
    <span class="d-acts">
      <!--
        Absent when the adapter reported no page (P5), which is the honest
        state for an item withdrawn upstream and for any source that has no
        per-item URL. A disabled button would claim there is somewhere to go.
      -->
      {#if webUrl}
        <button class="btn sm" onclick={() => void open(webUrl)}>
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M9 3h4v4M13 3 7.5 8.5" />
            <path d="M12 9.5V13H3V4h3.5" />
          </svg>
          Open in browser
        </button>
      {/if}
    </span>
    <button class="x" aria-label="Close panel" title="Close (Esc)" onclick={onclose}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
    </button>
  </div>

  <div class="d-b">
    {#if error}
      <!--
        A deep link into a corpus that has not synced yet is an ordinary event,
        not an error dialog — so it is a panel, it says which id it could not
        find, and it offers the way out.
      -->
      <div class="d-title">
        <div>
          <span class="k">{key}</span>
          <h2 id={titleId}>
            {error.code === "not_found"
              ? "Not in the local index"
              : error.code === "invalid"
                ? "That is not an entity address"
                : "Could not read this item"}
          </h2>
        </div>
      </div>
      <div class="sec">
        <p class="muted">{error.message}</p>
        <div class="acts">
          <button class="btn" onclick={onclose}>Close</button>
        </div>
      </div>
    {:else if !detail}
      <div class="d-title">
        <div>
          <span class="k">{key}</span>
          <h2 id={titleId}>Reading…</h2>
        </div>
      </div>
    {:else}
      {#if detail.deleted_at}
        <!--
          The source withdrew this item and knobas kept it: `sync.item` still
          holds the mirror row, `knobas.entity.deleted_at` carries the
          tombstone (interfaces §1's sweep), and links and notes may point at
          it (§5a). Saying so is the difference between a stale row and a
          deliberate one.
        -->
        <div class="prompt">
          <span class="pulse"></span>
          <span>
            Withdrawn upstream — last seen {ago(detail.deleted_at)}. Kept because links and
            notes may point at it.
          </span>
        </div>
      {/if}

      <div class="d-title">
        <div>
          <span class="k">
            <Monogram text={kindMonogram(shownKind, declared)} label={label} />
            {key}
          </span>
          <h2 id={titleId}>{detail.row.title}</h2>
        </div>
      </div>

      <div class="d-meta">
        <div>
          <div class="l">Source</div>
          <div class="v">{detail.source.display_name}</div>
        </div>
        <div>
          <div class="l">Kind</div>
          <div class="v">{label}</div>
        </div>
        <div>
          <div class="l">Author</div>
          <div class="v">{detail.author ?? "—"}</div>
        </div>
        <div>
          <div class="l">Updated</div>
          <div class="v k">{detail.row.updated_at ? ago(detail.row.updated_at) : "—"}</div>
        </div>
        <div>
          <div class="l">Synced</div>
          <div class="v k">{ago(detail.row.synced_at)}</div>
        </div>
      </div>

      {#if detail.body_text}
        <div class="sec">
          <div class="sec-h"><span class="lab">Description</span></div>
          <p class="d-body">{detail.body_text}</p>
        </div>
      {/if}

      <div class="sec">
        <div class="sec-h">
          <span class="lab">Details</span>
          <span class="k muted">{fields.length}</span>
        </div>
        <PayloadView {fields} />
      </div>

      <LinksPanel entityId={detail.row.entity_id} links={detail.links} />
      <HistoryPanel activity={detail.activity} />
    {/if}
  </div>
</aside>

<style>
  /*
    The `.d-h` is focusable so the reader lands inside the panel when it opens.
    It is not a control, so it takes no focus ring of its own — the panel
    heading right under it is what they are being shown.
  */
  .d-h:focus-visible {
    outline: none;
  }

  .acts {
    margin-top: 12px;
    display: flex;
    gap: 6px;
  }

  /*
    Body text is a source system's prose: newlines are meaningful, and one long
    unbroken token (a URL, a stack frame) must wrap rather than widen the panel.
  */
  .d-body {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
