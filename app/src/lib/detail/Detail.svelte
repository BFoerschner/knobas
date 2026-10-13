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
  import { untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { EVENTS, ipcErrorMessage, isIpcError } from "../ipc";
  import {
    entityView,
    getEntity,
    sourceStatuses,
    promoteContext,
    reachableTransitions,
    submitWrite,
    unlink,
    type EntityDetail,
    type LinkEntry,
    type SourceView as SourceViewData,
    type TreeRead,
    type ViewAction,
  } from "../ipc/entity";
  import { discardWrite, pendingWrites, type BuildParameter, type QueuedWrite, type WriteOpPayload } from "../ipc/sources";
  import { openFreshContext } from "../shell/contexts.svelte";
  import Monogram from "../shell/Monogram.svelte";
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { hashFor } from "../shell/router.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { openExternal } from "../shell/open-external";
  import { ago } from "../shell/time";
  import { push } from "../shell/toasts.svelte";
  import { openWriteQueue } from "../shell/write-queue.svelte";
  import BranchCheckoutNote from "./BranchCheckoutNote.svelte";
  import BuildsSummary from "./BuildsSummary.svelte";
  import CheckoutPanel from "./CheckoutPanel.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
  import LinkDialog from "./LinkDialog.svelte";
  import { linkChanges } from "./links.svelte";
  import LinksPanel from "./LinksPanel.svelte";
  import PayloadView from "./PayloadView.svelte";
  import { projectPayload } from "./payload";
  import StorageBody from "./StorageBody.svelte";
  import SourceView from "./view/SourceView.svelte";
  import ActionDialog from "./view/ActionDialog.svelte";
  import { echoesAt, sendingMark } from "./view/sending";
  import { PLACEHOLDER_MS, sameAnswer, VIEW_PATIENCE_MS, viewMemory, type Loan } from "./view-read";
  import { actionName, actionPayload, answeredAsDrawn, sendsComments, type ActionValues } from "./view/actions";
  import { asReview, pendingComments, type PendingComment } from "./view/pending-comments.svelte";
  import { openState, queuedSetting, settingPayload, type SettingMark, type SettingsTarget } from "./view/settings";
  import {
    pageSections,
    replaceSectionBody,
    type PageSection,
  } from "./page-sections";
  import {
    pageCommentsOf,
    pageVersionOf,
    parseStorageFormat,
    storageBodyOf,
  } from "./storage-format";

  let {
    entityId,
    kind,
    contextLabel,
    onclose,
    onnavigate,
  }: {
    entityId: string;
    /** From the address. `null` for the `#/entity/<id>` alias. */
    kind: string | null;
    /** The room this was opened over, for the crumb. */
    contextLabel: string;
    onclose: () => void;
    /**
     * Go to an address — a linked entity's own detail (spec §2).
     *
     * A callback rather than the router itself: the shell owns navigation, and
     * a slide-over that wrote `location.hash` would be a second navigator.
     */
    onnavigate: (hash: string) => void;
  } = $props();

  let detail = $state<EntityDetail | null>(null);
  let error = $state<{ code: string; message: string } | null>(null);
  /** The generation of the newest request — see `Tile.svelte`. */
  let token = 0;

  $effect(() => {
    const id = entityId;
    const mine = ++token;
    // An entity seen moments ago (back, forward) is drawn as it was, at once,
    // and both reads below replace it in place only where they differ (#693).
    const held = viewMemory.recall(id);
    detail = held?.detail ?? null;
    error = null;
    sourceView = held?.view ?? null;
    viewFailure = null;
    viewAnswered = held !== null && held.view !== undefined;
    settingMarks = {};
    settingWrites = {};
    sending = [];
    made = {};
    resending = null;
    watchQueue(false);
    untrack(() => void adoptSending(id, mine));
    // Untracked: the effect reads back what it just wrote, and tracking that
    // would re-open the panel every time a view landed.
    untrack(() => {
      if (sourceView) void adoptQueuedSettings(sourceView, mine);
    });
    // Both reads at once: the view does not need the row, and starting it a
    // round trip later is a round trip more of placeholder. Reads the room
    // made ahead while the panel being left stayed drawn are these reads
    // (#703), and the time they have taken counts toward the waits.
    const loan = viewMemory.borrow(id);
    const getting = loan?.row ?? getEntity(id);
    const timers = readView(id, mine, loan);
    void getting
      .then((answer) => {
        if (mine !== token) return;
        if (!sameAnswer(detail, answer)) detail = answer;
      })
      .catch((rejection) => {
        if (mine !== token) return;
        error = {
          code: isIpcError(rejection) ? rejection.code : "internal",
          message: ipcErrorMessage(rejection),
        };
      });
    return () => timers.forEach(clearTimeout);
  });

  // -- the source view (#661, ADR-0020) ---------------------------------------
  //
  // What the entity's own adapter draws for its kind, read live beside the
  // mirror's row. Three answers, each drawn differently: a view
  // replaces the generic projection with tabs and moves it behind *Raw*;
  // `null` leaves this panel exactly as it was; a failure leaves it too, under
  // one line saying why (rule 2: never silent about an error, never noisy
  // about an absence). Until the answer lands the body waits under one line
  // (#693, `viewWaiting`), so it is drawn once rather than drawn and swapped;
  // a source remembered as drawing nothing for the kind does not wait.

  /** The view, once one was drawn. */
  let sourceView = $state<SourceViewData | null>(null);
  /** The entity whose builds list is open (#732); another entity's starts closed. */
  let buildsOpenFor = $state<string | null>(null);
  /** Why the view could not be read, when it could not. */
  let viewFailure = $state<string | null>(null);
  /** Whether the reader opened *Raw*; its projection is drawn only then. */
  let rawOpen = $state(false);
  /** Whether the source has answered the view read (a view, `None` or a failure). */
  let viewAnswered = $state(false);
  /** Whether the read has outlasted {@link VIEW_PATIENCE_MS}. */
  let viewOverdue = $state(false);
  /**
   * Whether the opening has outlasted {@link PLACEHOLDER_MS} (#703): until
   * then nothing says *Reading…*, so an answer that comes promptly is drawn
   * with no line before it.
   */
  let slow = $state(false);

  /**
   * Whether the body waits for the view rather than drawing the mirror's
   * (#693). A body drawn and then swapped for tabs is the jump this ends: the
   * description vanished and the panel moved under the reader. So while the
   * read is in flight the body is one placeholder line -- except for a source
   * that answered `None` for this kind already this session, which draws its
   * generic body at once, and except past the patience, when the mirror is
   * drawn under its line and a late view still replaces it.
   */
  const viewWaiting = $derived(
    detail !== null &&
      !viewAnswered &&
      !viewOverdue &&
      !viewMemory.drawsNothing(detail.source.id, detail.row.kind),
  );

  /**
   * What the panel draws, remembered for the next opening (#693): a view or
   * `None` once answered; the row alone while the read is in flight or after
   * it failed, so a return draws the row and reads the view again.
   */
  $effect(() => {
    if (detail === null || detail.row.entity_id !== entityId) return;
    const answered = viewAnswered && viewFailure === null;
    viewMemory.keep(entityId, { detail, view: answered ? sourceView : undefined });
  });

  /**
   * Ask the entity's source for its view. Keyed on `token`, the read
   * generation of the `get_entity` beside it: an answer for an address the
   * panel has left is dropped exactly as a stale entity read is. Not re-read
   * by `refresh()`: a link drawn or withdrawn changes the mirror's row, not
   * what the source draws. A write queued from the view is what re-reads it
   * (`rereadView`). `loan` holds the read the room started, and when (#703).
   * Answers its two timers, the placeholder's and the patience's, for the
   * opening effect to clear when the panel moves on.
   */
  function readView(
    id: string,
    mine: number,
    loan: Loan | null,
  ): ReturnType<typeof setTimeout>[] {
    rawOpen = false;
    const waited = loan ? Date.now() - loan.since : 0;
    // A wait already over is over in the first frame, not one timer later.
    viewOverdue = waited >= VIEW_PATIENCE_MS;
    slow = waited >= PLACEHOLDER_MS;
    const placeholder = setTimeout(() => {
      if (mine === token) slow = true;
    }, PLACEHOLDER_MS - waited);
    const patience = setTimeout(() => {
      if (mine === token) viewOverdue = true;
    }, VIEW_PATIENCE_MS - waited);
    void (loan?.view ?? entityView(id))
      .then((answer) => {
        if (mine !== token) return;
        viewAnswered = true;
        // The same answer as the one drawn from memory is not redrawn: the
        // open tab stays where the reader left it.
        if (sameAnswer(sourceView, answer)) return;
        sourceView = answer;
        if (answer) void adoptQueuedSettings(answer, mine);
      })
      .catch((rejection: unknown) => {
        if (mine !== token) return;
        viewAnswered = true;
        // Rule 2: a failed read draws the mirror under its line, even where
        // memory had drawn a view a moment ago.
        sourceView = null;
        viewFailure = ipcErrorMessage(rejection);
      })
      .finally(() => clearTimeout(patience));
    return [placeholder, patience];
  }

  /**
   * Read the source's view again without blanking the panel (#667).
   *
   * What a write from the view needs once it has **settled** (#752):
   * `submit_write` answers as soon as the write is queued, so the re-read
   * waits for the queue to say the write went (`settleOnce`) -- a merge that
   * went is then at the source, and a view that still offered *Merge…* would
   * be offering what has happened. Kept on `token` like `refresh()`; a failed
   * re-read keeps the view that was drawn and says so.
   */
  async function rereadView() {
    const mine = token;
    try {
      const answer = await entityView(entityId);
      if (mine !== token) return;
      viewAnswered = true;
      viewFailure = null;
      if (!sameAnswer(sourceView, answer)) sourceView = answer;
    } catch (rejection) {
      if (mine !== token) return;
      push({ text: `Could not re-read this item's view: ${ipcErrorMessage(rejection)}`, tone: "err" });
    }
  }

  /**
   * Queue a view's action against this entity (#667, ADR-0020 rule 4).
   *
   * The payload is `actions.ts`'s to build; this panel only queues it, says
   * so, and reads the mirror and the view again. Answers whether the write was
   * queued, so the action's dialog stays up on a refusal.
   *
   * An action that sends the reader's pending line comments (#709) takes them
   * with it, and once it is queued those it took are no longer pending (one
   * written or edited while it was queued stays); a refused one keeps them.
   */
  async function queueAction(
    action: ViewAction,
    values: ActionValues,
    changed: BuildParameter[] = [],
    replaces: number | null = null,
  ): Promise<boolean> {
    const name = actionName(action.label);
    const entity = entityId;
    const taken = sendsComments(action) ? pendingComments.of(entity) : [];
    const payload = actionPayload(action, entity, values, changed, asReview(taken));
    // An edit sent back unchanged has nothing to write, and that is no fault
    // (#706): the dialog closes and says so.
    if (payload === null && answeredAsDrawn(action, values)) {
      push({ text: `Nothing changed, so no ${name.toLowerCase()} was queued` });
      return true;
    }
    if (payload === null) {
      push({ text: `Could not queue the ${name.toLowerCase()}: knobas cannot send it`, tone: "err" });
      return false;
    }
    const mine = token;
    let write: QueuedWrite;
    try {
      write = await submitWrite(payload);
    } catch (rejection) {
      push({ text: `Could not queue the ${name.toLowerCase()}: ${ipcErrorMessage(rejection)}`, tone: "err" });
      return false;
    }
    pendingComments.settle(entity, taken);
    push({ text: `${name} queued for ${key}` });
    // The dialog closes now (#752): the write is on the queue, and the panel
    // draws it as on its way until the queue says it settled.
    track(mine, write, payload, { action, values, changed, name, entity, taken });
    // *Edit and resend* replaces (the deputy's ruling on #752): the held or
    // refused original is withdrawn once its successor is queued, so the
    // queue holds one write per resend. A reader who withdrew it in the queue
    // meanwhile already did this; the refusal that answers is nothing to tell.
    if (replaces !== null) await discardWrite(replaces).catch(() => {});
    return true;
  }

  // -- writes on their way (#752) ----------------------------------------------
  //
  // `submit_write` answers once the write is queued; the send happens behind
  // the reader. Until it settles the panel draws what it will make, muted and
  // marked (`view/sending.ts`), from the queue's own rows -- those this panel
  // queued and, on opening, those `pending_writes` still holds for this
  // entity, so a restart mid-send draws them too. `activity:new` follows every
  // queue transition; on each, the queue is asked where these writes are
  // (`settleOnce`). One that went is re-read into the view and its echo
  // removed in the same step, so the real comment replaces the pending one
  // with nothing blank between. One held or refused loses its echo and says
  // why in a toast, with the way to the queue and -- for a write made here --
  // the dialog again with what the reader typed.

  /** What a write queued from this panel was made of, for *Edit and resend*. */
  interface Made {
    action: ViewAction;
    values: ActionValues;
    changed: BuildParameter[];
    name: string;
    /** The entity its line comments were pending on, and those it took. */
    entity: string;
    taken: PendingComment[];
  }

  /** This panel's writes on their way, oldest first, as the queue answered them. */
  let sending = $state<QueuedWrite[]>([]);
  /** By write id: what each write queued from here was made of. A plain map, read on settle. */
  let made: Record<number, Made> = {};
  /** The dialog reopened by *Edit and resend*, with what was typed, and the write it replaces. */
  let resending = $state<(Made & { replaces: number }) | null>(null);

  /** The newest title a write on its way gives this entity, if any. */
  const newTitle = $derived(echoesAt(sending, "title").at(-1) ?? null);

  /** Words on their way to the discussion, drawn at the end of the view's last tab. */
  const saidOnTheirWay = $derived(
    sourceView === null
      ? []
      : echoesAt(sending, "said").map(({ write, echo }) => ({
          id: write.id,
          verdict: echo.verdict,
          text: echo.text,
          mark: sendingMark(write),
        })),
  );

  /** Every other echo, and a comment's where no view is drawn, as one line each. */
  const sendingLines = $derived([
    ...echoesAt(sending, "line").map(({ write, echo }, i) => ({
      key: `${write.id}:l${i}`,
      text: echo.text,
      mark: sendingMark(write),
    })),
    ...(sourceView === null
      ? echoesAt(sending, "said").map(({ write, echo }) => ({
          key: `${write.id}:s`,
          text: echo.verdict ? `Review (${echo.verdict}): ${echo.text}` : `Comment: ${echo.text}`,
          mark: sendingMark(write),
        }))
      : []),
  ]);

  /** This entity's own settings in the view: what a reopened dialog offers again. */
  const viewSettings = $derived(
    (sourceView?.sections ?? [])
      .flatMap((section) => section.blocks)
      .flatMap((block) => (block.type === "settings" && block.entity === entityId ? block.values : [])),
  );

  /**
   * Draw `write` as on its way and watch the queue for it. The row is the
   * queue's answer; `payload` stands in only where an answer carries none.
   * Asks the queue once at once too: a send quick enough to settle before the
   * watch was listening announced itself to nobody.
   */
  function track(mine: number, write: QueuedWrite, payload: WriteOpPayload, from?: Made) {
    if (mine !== token) return;
    if (from) made[write.id] = from;
    if (!sending.some((w) => w.id === write.id)) sending = [...sending, { ...write, payload: write.payload ?? payload }];
    watchQueue(true);
    void settleSettings(mine);
  }

  /** On opening: this entity's writes the queue still sends, drawn as on their way. */
  async function adoptSending(id: string, mine: number) {
    let open: QueuedWrite[];
    try {
      open = await pendingWrites();
    } catch {
      return;
    }
    if (mine !== token) return;
    const mineToDraw = open
      .filter((w) => w.entity_id === id && w.state === "pending" && !sending.some((s) => s.id === w.id))
      .sort((a, b) => a.id - b.id);
    if (mineToDraw.length === 0) return;
    sending = [...mineToDraw, ...sending];
    watchQueue(true);
  }

  /** A write that will not go on its own: its echo goes, and a toast says why. */
  function stopped(write: QueuedWrite) {
    const from = made[write.id];
    delete made[write.id];
    // Nothing typed is lost: line comments a refused review took are pending again.
    for (const { path, side, line, body } of from?.taken ?? []) {
      pendingComments.add(from!.entity, { path, side, line, body });
    }
    const name = from?.name ?? write.op.replace(/_/g, " ");
    const why = write.state === "held" ? "was held" : "was refused";
    push({
      text: `${name} ${why}${write.detail ? `: ${write.detail}` : ""}`,
      tone: "err",
      ms: 20_000,
      action: { label: "Open write queue", run: openWriteQueue },
      more: from ? [{ label: "Edit and resend", run: () => (resending = { ...from, replaces: write.id }) }] : [],
    });
  }

  // -- a setting edited from the view (#668, ADR-0020 rule 4) ----------------
  //
  // A settings block's edit is a write like an action's, with one more
  // promise: the row says where its write is until the queue settles it. A
  // write that went on the spot is read back at once. One that waits (the
  // source unreachable), is held or refused keeps the queue's own state as
  // its mark, and the queue is watched -- `activity:new` follows every queue
  // transition -- until the write is no longer open, when the view is read
  // again and shows what the source now holds.

  /** The edits on their way, by setting name. */
  let settingMarks = $state<Record<string, SettingMark>>({});
  /** The queued write behind each mark, by name; a plain map, read on events. */
  let settingWrites: Record<string, number> = {};
  /** Whether the queue is being watched, and how to stop. */
  let watching = false;
  let unwatch: (() => void) | null = null;

  /** Start or stop watching the queue for this panel's open setting writes. */
  function watchQueue(on: boolean) {
    if (on === watching) return;
    watching = on;
    if (!on) {
      unwatch?.();
      unwatch = null;
      return;
    }
    listen(EVENTS.activityNew, () => void settleSettings(token))
      .then((stop) => {
        if (watching && unwatch === null) unwatch = stop;
        else stop();
      })
      .catch(() => {
        // No event channel: the marks stay until the panel is opened again.
        watching = false;
      });
  }

  $effect(() => () => watchQueue(false));

  /**
   * Queue `name = value` on the block's entity, edited against `base` (#668).
   * Answers whether the write was queued, so the editor stays open on a
   * refusal.
   */
  async function saveSetting(
    block: SettingsTarget,
    name: string,
    value: string,
    base: string | null,
  ): Promise<boolean> {
    const payload = settingPayload(block.write_op, block.entity, name, value, base);
    if (payload === null) {
      push({ text: `Could not save ${name}: knobas cannot send this change`, tone: "err" });
      return false;
    }
    const mine = token;
    settingMarks[name] = { state: "saving", detail: null, value };
    let write;
    try {
      write = await submitWrite(payload);
    } catch (rejection) {
      // Another item opened meanwhile: its row of this name is not this one.
      if (mine !== token) return false;
      delete settingMarks[name];
      push({ text: `Could not save ${name}: ${ipcErrorMessage(rejection)}`, tone: "err" });
      return false;
    }
    if (mine !== token) return true;
    settingWrites[name] = write.id;
    // Watched from here, not from the first answer: if that read fails, a
    // queue movement is what asks again, and nothing would hear one.
    watchQueue(true);
    await settleSettings(mine);
    if (mine !== token) return true;
    // Settled by this read or by a queue movement that raced it: either way
    // the write is no longer open, which is what "set" says.
    if (settingWrites[name] !== write.id && settingMarks[name] === undefined) push({ text: `${name} set` });
    else if (settingMarks[name]?.state === "pending") push({ text: `${name} queued` });
    return true;
  }

  /**
   * Mark the rows whose edits are still on the queue (#692).
   *
   * A detail closed while a setting's write waited loses its marks with it;
   * the write does not leave the queue. So when a view is drawn, every open
   * write of a settings block's op, aimed at the block's entity and naming one
   * of its rows, marks that row and is watched as a fresh save is. The newest
   * write for a row wins, as a second save would. A row saved from this panel
   * meanwhile keeps its own write, even one `submit_write` has not answered.
   */
  async function adoptQueuedSettings(view: SourceViewData, mine: number) {
    const blocks = view.sections.flatMap((s) => s.blocks).filter((b) => b.type === "settings");
    if (blocks.length === 0) return;
    let open;
    try {
      open = await pendingWrites();
    } catch {
      // Unmarked rows, as before #692; a save asks the queue again.
      return;
    }
    if (mine !== token) return;
    const found: Record<string, { id: number; mark: SettingMark }> = {};
    for (const block of blocks) {
      const names = new Set(block.values.map((v) => v.name));
      for (const write of open) {
        const edit = queuedSetting(block.write_op, write);
        const state = openState(write);
        if (edit === null || state === null || edit.entity !== block.entity || !names.has(edit.name)) continue;
        if ((found[edit.name]?.id ?? -1) > write.id) continue;
        found[edit.name] = { id: write.id, mark: { state, detail: write.detail, value: edit.value } };
      }
    }
    for (const [name, { id, mark }] of Object.entries(found)) {
      // A row saved from this panel meanwhile, queued or still being queued, keeps its save.
      if (settingWrites[name] !== undefined || settingMarks[name] !== undefined) continue;
      settingWrites[name] = id;
      settingMarks[name] = mark;
    }
    if (Object.keys(settingWrites).length > 0) watchQueue(true);
  }

  /** Settles run one after another, so two never re-read the view for one write. */
  let settling: Promise<void> = Promise.resolve();
  function settleSettings(mine: number): Promise<void> {
    settling = settling.then(() => settleOnce(mine));
    return settling;
  }

  /**
   * Ask the queue where this panel's setting writes are. Each still open
   * takes its state as its mark; each gone (sent, or withdrawn in the queue)
   * loses its mark once the view has been read again -- unless the reader
   * saved that row again meanwhile, whose newer write keeps its own mark.
   */
  async function settleOnce(mine: number): Promise<void> {
    if (mine !== token || (Object.keys(settingWrites).length === 0 && sending.length === 0)) return;
    let open;
    try {
      open = await pendingWrites();
    } catch {
      // The marks stay; the next queue movement asks again.
      return;
    }
    if (mine !== token) return;
    // Each landed name with the mark it had: a save of that row made while
    // the view is re-read replaces the mark, and the newer one stays.
    const landed: [string, SettingMark | undefined][] = [];
    for (const [name, id] of Object.entries(settingWrites)) {
      const row = open.find((w) => w.id === id);
      const state = openState(row);
      if (state !== null) {
        settingMarks[name] = { state, detail: row?.detail ?? null, value: settingMarks[name]?.value ?? "" };
      } else {
        landed.push([name, settingMarks[name]]);
        delete settingWrites[name];
      }
    }
    // The writes on their way: still pending keeps its echo (with its newest
    // reason), held or refused loses it now, gone (sent, or discarded in the
    // queue) keeps it until the re-read below has drawn what it made.
    const went: number[] = [];
    const still: QueuedWrite[] = [];
    for (const write of sending) {
      const row = open.find((w) => w.id === write.id);
      if (row === undefined) {
        went.push(write.id);
        still.push(write);
      } else if (row.state === "pending") {
        still.push({ ...row, payload: row.payload ?? write.payload });
      } else {
        stopped(row);
      }
    }
    if (still.length !== sending.length || still.some((w, i) => w !== sending[i])) sending = still;
    watchQueue(Object.keys(settingWrites).length > 0 || sending.length > 0);
    if (landed.length === 0 && went.length === 0) return;
    await refresh();
    if (mine !== token) return;
    await rereadView();
    if (mine !== token) return;
    for (const [name, mark] of landed) {
      if (settingMarks[name] === mark) delete settingMarks[name];
    }
    if (went.length > 0) {
      for (const id of went) delete made[id];
      sending = sending.filter((w) => !went.includes(w.id));
      watchQueue(Object.keys(settingWrites).length > 0 || sending.length > 0);
    }
  }

  /**
   * Re-read this entity without blanking the panel.
   *
   * What a *write* needs: after an unlink the links array is stale, and the
   * reader is looking at the row that has to disappear. The mount effect above
   * cannot do it — it clears `detail` first, so the whole slide-over would
   * flash "Reading…" for one round trip after every small action.
   *
   * `token` is read and not bumped: this is the same read generation as the
   * effect that opened the panel, so an answer that lands after the address
   * moved on is dropped exactly as a slow first read would be.
   */
  async function refresh() {
    const mine = token;
    try {
      const answer = await getEntity(entityId);
      if (mine !== token) return;
      detail = answer;
    } catch (rejection) {
      push({ text: `Could not re-read this item: ${ipcErrorMessage(rejection)}`, tone: "err" });
    }
  }

  /**
   * Re-read when something outside this panel drew a link.
   *
   * The launcher's `Tab` chain can link the entity this slide-over has open,
   * and the write happens in the shell. Without this the panel would keep
   * saying what it said before the link — a state the app has already left.
   *
   * `seen` is a plain `let`, so writing it cannot re-trigger the effect; the
   * first run only records where the counter stood when the panel opened.
   */
  let seenLinkChanges = linkChanges.count;
  $effect(() => {
    const now = linkChanges.count;
    if (now === seenLinkChanges) return;
    seenLinkChanges = now;
    void refresh();
  });

  /**
   * Whether *Link to…* is up.
   *
   * The dialog is `Modal`-based, so it takes rung 1 of the Esc ladder: one
   * press closes it and hands focus back to the button that opened it, and the
   * slide-over underneath stays open.
   */
  let linking = $state(false);

  /**
   * Withdraw a link, and show the result.
   *
   * No confirmation: re-linking the same pair is one action, so the removal is
   * cheap to reverse (§5a's partial unique index is what makes that true).
   *
   * Unless the link is `implied` — drawn by knobas from a `[[reference]]` in
   * a note's body (#46). The body is the source of truth for those, so
   * tombstoning the row here would be undone by that note's next autosave,
   * silently; `NoteView` refuses the same unlink from the note's own end.
   * The other end of an implied link is the note, so the refusal can say
   * where to go.
   */
  async function removeLink(entry: LinkEntry) {
    if (entry.link.origin === "implied") {
      push({
        text: `This link comes from a [[reference]] in “${entry.other.title || entry.other.entity_id}”. Remove the reference there to withdraw it.`,
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
  /**
   * Where this item sits inside its source, when its record says — a
   * Confluence page's ancestor path (#284).
   *
   * Read off the **row**, not out of `detail.payload`, although the payload is
   * right there. The launcher row draws the same string, the launcher has no
   * payload to read it out of, and one rule with two implementations is the
   * drift #277 spent a whole test file pinning against. So the one spelling is
   * `knobas_core::path_line_read!` in SQL, both surfaces read what it
   * joined, and this line is a field access rather than a second rule. It
   * misses to `null`, so a kind that declares no parent and a page nobody has
   * filed anywhere both show no path rather than a wrong one.
   */
  const path = $derived(detail?.row.path ?? null);
  /** Narrowed once, so the button and its handler agree that it is a string. */
  const webUrl = $derived(detail?.web_url ?? null);

  /**
   * The page's own body, as nodes — a Confluence page and nothing else (#285).
   *
   * `null` for every other item, and that is the whole gate: the read is
   * `storageBodyOf`, which answers only for the adapter whose payloads carry a
   * storage format, and this panel falls back to the normalized `body_text`
   * below exactly as it always did. A markup dialect is not something #277's
   * `KindPaths` can declare yet — see `storage-format.ts` for why the interim
   * read is shaped this way.
   *
   * Parsed here rather than inside `StorageBody`, so the component takes a
   * tree and has no opinion about where markup comes from — and so the
   * fallback can be decided by whether there *is* a tree.
   */
  const pageStorage = $derived(storageBodyOf(detail?.source.adapter_kind, detail?.payload));

  const pageBody = $derived(pageStorage === null ? null : parseStorageFormat(pageStorage));

  /**
   * The page's body cut into sections, each with its own rendered tree (#286).
   *
   * The **string** is what a section carries offsets into, so the cut is made
   * over `pageStorage` and each part is parsed from its own slice. The result
   * is the same nodes {@link pageBody} would have produced, split at the
   * headings -- which is what lets an *Edit* sit beside one heading's prose
   * rather than over the whole page.
   *
   * `preamble` is what comes before the first heading. It belongs to no
   * section and is therefore not editable: a paragraph with no heading has
   * nothing an *Edit* could name.
   */
  const pageParts = $derived.by(() => {
    if (pageStorage === null) return null;
    const sections = pageSections(pageStorage);
    return {
      preamble: parseStorageFormat(pageStorage.slice(0, sections[0]?.start ?? pageStorage.length)),
      sections: sections.map((section) => ({
        section,
        nodes: parseStorageFormat(pageStorage.slice(section.start, section.end)),
      })),
    };
  });

  /**
   * The version the mirror holds for this page, which is what an edit is made
   * **against** -- `null` when the record does not say.
   *
   * The gate on offering an edit at all, and the direction is deliberate: an
   * `UpdatePage` sent with a guessed `base_version` would either overwrite
   * somebody's work or be refused by Confluence for a reason the reader cannot
   * act on. No version, no *Edit* (#286).
   */
  const pageVersion = $derived(pageVersionOf(detail?.source.adapter_kind, detail?.payload));

  /**
   * The page's comments, in the order its record carries them (#284, #285).
   *
   * A comment is not a kind of its own: it rides in its page's payload the way
   * a Jira comment rides in its issue's, so this is a read of that payload and
   * not a second round trip. Empty for every other item.
   *
   * Parsed **here** and not in the `{#each}`, the same shape as the body
   * above: a `parseStorageFormat` call in the template re-runs on every
   * re-render of this panel -- a fallback offer landing, a link being drawn --
   * and re-parsing a discussion to answer a question nobody asked is work the
   * derived value is for.
   */
  const pageComments = $derived(
    pageCommentsOf(detail?.source.adapter_kind, detail?.payload).map((comment) => ({
      id: comment.id,
      author: comment.author,
      when: comment.when,
      nodes: parseStorageFormat(comment.storage),
    })),
  );

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

  // -- the status select (#179, #498) -----------------------------------------
  //
  // Moving a ticket without leaving knobas. It was optimistic by construction
  // until #498: knobas had no read of which transitions a ticket's workflow
  // offers from where it stands, so the select offered whatever the source's
  // *corpus* had been seen to use and the reader learnt which of those were
  // real from a refusal in the write queue. `reachableTransitions` is that
  // read, and the select now offers its answer and nothing else.
  //
  // **Two answers, and each says something the other cannot.** The ticket says
  // where it *stands* -- the mirrored status (#760: `get_entity` carries it),
  // which is what the select shows and what a move is measured against -- and
  // `source_statuses` is the corpus offer that is still the fallback when the
  // workflow read fails. The workflow read says where it can *go*, which no
  // mirror holds and nothing stores.
  //
  // The write is unchanged: `Transition` still names the status, the adapter
  // still resolves that name against the source's own answer at flush time, and
  // a move refused between the read and the flush still comes back by name
  // through the queue. This read narrows the offer; it does not replace that
  // resolution, and treating it as a guarantee is exactly what the queue's
  // refusal exists to catch.

  /** The source half of the address is the source id (interfaces §4.1). */
  const sourceId = $derived(entityId.slice(0, entityId.indexOf(":")));

  /**
   * Whether this panel is showing a ticket — the only kind that can move.
   *
   * A `$derived` and not the comparison written inline below, for two reasons.
   * It changes only when the *answer* flips, so a `refresh()` that re-reads the
   * same ticket does not re-issue the fallback read; and it is the effect's gate,
   * so a read a note or a page can never use is never made.
   */
  const isTicket = $derived(shownKind === "ticket");

  /**
   * Whether this entity has a checkout to show (#499).
   *
   * A repo has one; a branch shows its repo's, because a branch is a ref inside
   * the same working tree and spec #491 story 29 puts the same buttons on both.
   * Nothing else does, and `entity_checkout` refuses any other kind by name --
   * so this list and `knobas_app::checkout::CHECKOUT_KINDS` are one rule, and
   * asking on a ticket would be the panel drawing a refusal.
   */
  const hasCheckout = $derived(shownKind === "repo" || shownKind === "branch");

  /**
   * Where the checkout's working tree stands, as the panel last read it, and
   * for which entity (#704). Drawn only while that entity is the one shown, so
   * the previous detail's read never sits under this one's title.
   */
  let localTree = $state<{ entityId: string; tree: TreeRead | null } | null>(null);

  /**
   * The ticket's current status: its **mirrored** one, read by `get_entity`
   * beside the row (#760) by the rule the board reads it by. Reading it out of
   * `detail.payload` here instead would be a second payload read, in the shell,
   * spelling `fields.status.name` a second time; ADR-0007 exists to stop
   * exactly that. Before #760 it was found among the cards of a whole read of
   * the source's board, 725 KB per open at 5,000 tickets.
   */
  const currentStatus = $derived(detail?.status ?? null);

  /**
   * Whether this source can move a ticket at all.
   *
   * Read off the adapter's declared `write_ops`, which is what the action bar
   * is drawn from everywhere else: `submit_write` refuses an op the source does
   * not declare, and the surface that offered it is what is at fault. It is
   * also the gate on the workflow read below -- a source with no `transition`
   * has no workflow to read and refuses the call by name (battery clause 8), so
   * making it would be asking a question whose answer is known.
   */
  const declaresTransition = $derived(
    kindRegistry.writeOps(detail?.source.adapter_kind ?? "").includes("transition"),
  );

  /**
   * What this ticket's workflow offers from where it stands, or `null` while
   * nothing has been read (#498).
   *
   * `null` and an empty array are two different answers and must stay that
   * way: `null` is "not read, or the read failed", which falls back to the
   * corpus offer below; `[]` is a workflow that answered and has nowhere left
   * to go, which draws no select at all.
   */
  let reachable = $state<string[] | null>(null);

  /**
   * Whether the workflow read was *tried and failed*, which is the only thing
   * that puts *offer unverified* on screen.
   *
   * Not the same as `reachable === null`: that is also true for the moment
   * before the answer lands, and a note that flickered on during every open
   * would be telling the reader something untrue about a read still in flight.
   */
  let unverifiedFor = $state<string | null>(null);

  /**
   * Keyed by the entity whose read failed (#760), so it is `false` the moment
   * another ticket opens, whatever order the effects below run in: the
   * fallback read never sees the previous ticket's `true`.
   */
  const offerUnverified = $derived(unverifiedFor === entityId);

  /** The workflow read's own generation -- `boardToken`'s reason, twice over. */
  let workflowToken = 0;

  $effect(() => {
    const id = entityId;
    const mine = ++workflowToken;
    reachable = null;
    unverifiedFor = null;
    if (!isTicket || !declaresTransition) return;
    void reachableTransitions(id)
      .then((statuses) => {
        if (mine === workflowToken) reachable = statuses;
      })
      .catch(() => {
        // **Swallowed, and this is the one swallow that shows.** A workflow
        // read can fail for reasons the reader can do nothing about from here
        // -- a credential that expired, a Jira that is down -- and a toast per
        // opened ticket would be noise. What the reader is owed is that the
        // offer on screen is then the old optimistic one, which is what the
        // note beside the select says.
        if (mine === workflowToken) unverifiedFor = id;
      });
  });

  /**
   * The statuses this source's corpus shows -- the offer's fallback (#179) --
   * read only once the workflow read has failed and the fallback is wanted
   * (#760), and `null` until it lands.
   */
  let corpusOffer = $state<string[] | null>(null);

  /**
   * The corpus read's own generation.
   *
   * **Not `token`.** That one belongs to `get_entity`, and bumping it here
   * would make this effect invalidate the panel's own read.
   */
  let boardToken = 0;

  $effect(() => {
    const source = sourceId;
    const wanted = offerUnverified;
    const mine = ++boardToken;
    corpusOffer = null;
    if (!source || !isTicket || !wanted) return;
    // Scoped to the ticket's own source and to nothing else: what this reads
    // is the statuses that *source corpus* shows, and narrowing it by the room
    // would offer a move only where it had already been made.
    void sourceStatuses(source)
      .then((statuses) => {
        if (mine === boardToken) corpusOffer = statuses;
      })
      .catch(() => {
        // Swallowed on purpose: the fallback is an extra a ticket detail can
        // do without, and a toast about a list nobody asked to see would be
        // noise over the item they did. No offer, so no select.
      });
  });

  /**
   * The statuses the select offers, in the order their source gave them, with
   * the one this ticket is already in taken out -- it is rendered separately,
   * marked and unselectable, the way the terminal group already was.
   *
   * Three answers, not two, and the third is why this is a `$derived.by`:
   *
   * * the **workflow answered**, and its answer is the offer. **Never merged
   *   with the corpus**: a corpus status the workflow does not reach is exactly
   *   what this read exists to stop offering, and a union would put every one
   *   of them back;
   * * the read was **tried and failed**, and the corpus offer is the fallback,
   *   labelled *offer unverified* beside the select;
   * * nothing has answered **yet**, and the honest offer is none. The corpus is
   *   the fallback for a read that failed, not for one still in flight: showing
   *   it here would offer statuses nobody has checked in the one window where
   *   the note that says so has not been earned.
   */
  const offered = $derived.by(() => {
    const offer = reachable ?? (offerUnverified ? (corpusOffer ?? []) : []);
    return offer.filter((status) => status !== currentStatus);
  });

  /**
   * Whether this ticket can be moved from here.
   *
   * Three things have to be true, and each absence is honest rather than a
   * disabled control: it is a ticket, its adapter declares the `transition`
   * write op, and there is at least one status to move to. The third now
   * carries a second case it did not have before #498 -- a workflow that
   * answered with nothing -- and the same drawing is right for it: a select
   * whose only entry is the status the ticket is already in is a control with
   * nothing to do.
   */
  const canTransition = $derived(isTicket && declaresTransition && offered.length > 0);

  /** True while a move is being queued, so the select cannot double-fire. */
  let moving = $state(false);

  /**
   * Queue a move.
   *
   * **Nothing here changes what the select shows.** The board reflects the
   * mirror, so the ticket's status is still what the source last said until the
   * write lands and a sync mirrors it; a select that jumped to the new value
   * would be reporting a hope, and the queue may yet hold or refuse this. The
   * `select` element is re-bound to the mirrored status for the same reason --
   * see the `value` in the markup.
   */
  async function move(event: Event) {
    const select = event.currentTarget;
    if (!(select instanceof HTMLSelectElement)) return;
    const status = select.value;
    // Put the control back where the mirror has it, before the await: the
    // reader must never be left looking at a status nothing has recorded.
    select.value = currentStatus ?? "";
    if (!status || status === currentStatus || moving) return;
    moving = true;
    const mine = token;
    try {
      const payload: WriteOpPayload = { Transition: { entity: entityId, status } };
      track(mine, await submitWrite(payload), payload);
      push({ text: `Move to ${status} queued for ${key}` });
    } catch (rejection) {
      push({ text: `Could not queue the move: ${ipcErrorMessage(rejection)}`, tone: "err" });
    } finally {
      moving = false;
    }
  }

  // -- the page section edit and the page comment (#286) ---------------------
  //
  // Spec #272's Confluence half, and the two affordances it asks for: one
  // section of prose at a time, and a reply. Both are queued writes like every
  // other -- `submitWrite` puts them on the write queue, which decides send,
  // pend or hold, and nothing here waits for a source.

  /**
   * Whether this page's sections say anything about editing at all.
   *
   * Two things: it is a page whose markup this app can read, and its adapter
   * declares `update_page` (`submit_write` refuses one that does not, and the
   * surface that offered it is what is at fault). When this is false the page
   * renders exactly as it did before #286 -- no offer, and no explanation of
   * an offer nobody was expecting.
   */
  const pageWritesOffered = $derived(
    pageStorage !== null &&
      kindRegistry.writeOps(detail?.source.adapter_kind ?? "").includes("update_page"),
  );

  /**
   * Whether an edit can actually be *made*, which is the narrower question.
   *
   * The record has to say what version it stands at: that is what the edit is
   * made against, and an `UpdatePage` sent with a guessed `base_version` would
   * either overwrite somebody's work or be refused for a reason the reader
   * cannot act on.
   *
   * Kept apart from {@link pageWritesOffered} deliberately. Folding the two
   * would take the **refusal** away with the offer -- a page with no readable
   * version would show no *Edit*, no sentence saying why, and no way to the
   * wiki, which is a section that silently offers nothing rather than one that
   * refuses.
   *
   * A *section* may still refuse itself -- see `PageSection.refusal` -- and
   * that is a per-section fact this flag does not carry.
   */
  const canEditPage = $derived(pageWritesOffered && pageVersion !== null);

  /** Whether a comment may be added to this page from here. */
  const canCommentOnPage = $derived(
    pageStorage !== null &&
      kindRegistry.writeOps(detail?.source.adapter_kind ?? "").includes("comment"),
  );

  /** The section being edited, by its index, or `null`. One at a time. */
  let editingSection = $state<number | null>(null);
  /** What the reader has typed into the open section editor. */
  let sectionDraft = $state("");
  /** True while a section edit is being queued, so the button cannot double-fire. */
  let queueingSection = $state(false);

  /** Open the editor on one section, prefilled with the section as it stands. */
  function editSection(section: PageSection) {
    editingSection = section.index;
    sectionDraft = section.text;
  }

  /**
   * Queue the edit as a whole-body `UpdatePage`.
   *
   * **The whole body**, re-assembled around the edited section: Confluence's
   * content `PUT` replaces the record, so a write carrying one section would
   * delete the rest of the page. `replaceSectionBody` copies everything
   * outside the section byte for byte, macros and tables included.
   *
   * `base_version` is the version the **mirror** holds, which is the version
   * the reader was looking at when they typed. If the page has moved on since,
   * the write is held with both versions side by side; if it moves on between
   * the last sync and the flush, Confluence itself aborts. Neither is this
   * function's job, and that is the point.
   */
  async function queueSectionEdit(section: PageSection) {
    if (pageStorage === null || pageVersion === null || queueingSection) return;
    queueingSection = true;
    const mine = token;
    try {
      const payload: WriteOpPayload = {
        UpdatePage: {
          entity: entityId,
          base_version: pageVersion,
          body: replaceSectionBody(pageStorage, section, sectionDraft),
        },
      };
      track(mine, await submitWrite(payload), payload);
      push({ text: `Edit to ${section.heading} queued for ${key}` });
      editingSection = null;
      sectionDraft = "";
    } catch (rejection) {
      push({ text: `Could not queue the edit: ${ipcErrorMessage(rejection)}`, tone: "err" });
    } finally {
      queueingSection = false;
    }
  }

  /** True while the comment box is open. */
  let commenting = $state(false);
  /** What the reader has typed into the comment box. */
  let commentDraft = $state("");
  /** True while a comment is being queued. */
  let queueingComment = $state(false);

  /**
   * Queue a comment on this page.
   *
   * The op is the SPI's own `Comment` with the **page** as its container --
   * the same act as a reply on a ticket, so it is the same op. What the reader
   * typed goes over as text and the adapter renders it into storage format,
   * which is where the escaping lives: a `<` in somebody's prose must reach
   * the wiki as a `<` and not as the start of an element.
   */
  async function queueComment() {
    if (commentDraft.trim() === "" || queueingComment) return;
    queueingComment = true;
    const mine = token;
    try {
      const payload: WriteOpPayload = { Comment: { entity: entityId, body: commentDraft } };
      track(mine, await submitWrite(payload), payload);
      push({ text: `Comment queued for ${key}` });
      commenting = false;
      commentDraft = "";
    } catch (rejection) {
      push({ text: `Could not queue the comment: ${ipcErrorMessage(rejection)}`, tone: "err" });
    } finally {
      queueingComment = false;
    }
  }

  /** True while a promotion is in flight, so the button cannot double-fire. */
  let promoting = $state(false);

  /**
   * Promote this ticket to a context of its own and go there (spec §7, #47).
   *
   * Idempotent on the backend, so pressing it on an already-promoted ticket
   * simply lands in the existing room.
   */
  async function promote() {
    if (promoting) return;
    promoting = true;
    try {
      const row = await promoteContext(entityId);
      await openFreshContext(row, onnavigate);
    } catch (rejection) {
      push({ text: `Could not promote: ${ipcErrorMessage(rejection)}`, tone: "err" });
    } finally {
      promoting = false;
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
      <!--
        On a ticket and nowhere else, and *absent* rather than disabled on
        everything else -- the same rule *Open in browser* follows above: a
        disabled button would claim there is a flow to start where there is
        none. The address is the flow's identity, so this navigates rather than
        opening a dialog; whether a flow already exists is the stepper's
        question to answer, not this button's.
      -->
      {#if shownKind === "ticket"}
        <button class="btn sm" onclick={() => onnavigate(hashFor({ view: "start-work", key: entityId }))}>
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M5 3.5v9l7-4.5z" />
          </svg>
          Start work
        </button>
        <!--
          Same placement rule as *Start work*: on a ticket and nowhere else,
          absent rather than disabled elsewhere. Spec §7: "any ticket can be
          promoted to a context" — an epic is a ticket kind too, and the
          backend reads which of the two it is off the mirror.
        -->
        <button class="btn sm" disabled={promoting} onclick={() => void promote()}>
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M8 13V4M4.5 7.5 8 4l3.5 3.5" />
          </svg>
          Promote
        </button>
      {/if}
      <button class="btn sm" onclick={() => (linking = true)}>
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M6.5 9.5 9.5 6.5" />
          <path d="M7 4.5 8.5 3a2.5 2.5 0 0 1 3.5 3.5L10.5 8" />
          <path d="M9 11.5 7.5 13A2.5 2.5 0 0 1 4 9.5L5.5 8" />
        </svg>
        Link to…
      </button>
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
              ? "Not in the mirror"
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
          <!-- Silent, at its height, for the moment a prompt read takes (#703). -->
          <h2 id={titleId}>{slow ? "Reading…" : "\u00a0"}</h2>
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

      {#if !detail.source.enabled}
        <!--
          The other reason a reader hides an entity (issue #204): the user
          turned the source off, so every list dropped its items — but the
          detail reaches past that filter for the same §5a reason it reaches
          past tombstones, and an item that opened looking entirely ordinary
          would contradict every surface that says it does not exist. A
          second, independent banner rather than a variant of the one above:
          "withdrawn upstream" is the source's doing and permanent, this is
          the user's own and one click away from undone — and when both facts
          hold, both are true and both show.
        -->
        <div class="prompt">
          <span class="pulse"></span>
          <span>
            Its source is turned off — {detail.source.display_name} is disabled, so this item
            is hidden everywhere else. Re-enable the source to bring it back.
          </span>
        </div>
      {/if}

      <div class="d-title">
        <div>
          <span class="k">
            <Monogram text={kindMonogram(shownKind, declared)} label={label} />
            {key}
            <!-- Where a write on its way will leave it (#752): one chip each. -->
            {#each echoesAt(sending, "state") as { write, echo } (`${write.id}:${echo.to}`)}
              <span class="sending-chip" title="Queued; knobas is sending it">
                → {echo.to}<span class="sending-mark">{sendingMark(write)}</span>
              </span>
            {/each}
          </span>
          {#if newTitle}
            <h2 id={titleId} class="sending-title">
              {newTitle.echo.title}<span class="sending-mark">{sendingMark(newTitle.write)}</span>
            </h2>
          {:else}
            <h2 id={titleId}>{detail.row.title}</h2>
          {/if}
          {#if shownKind === "branch"}
            <!--
              A branch's title is its name: the Gitea adapter maps it so
              (`map.rs`, pinned by its own test), and it is the only source
              with branches. A source that titles a branch otherwise needs a
              declared name to compare here.
            -->
            <BranchCheckoutNote
              tree={localTree?.entityId === detail.row.entity_id ? localTree.tree : null}
              branch={detail.row.title}
            />
          {/if}
          {#if path}
            <div class="d-path">{path}</div>
          {/if}
        </div>
      </div>

      <div class="d-meta">
        <!--
          The mockup puts *Status* first in the meta grid, as a select
          (`signal-miller.html:2586`). Absent rather than disabled where a move
          is not possible, the rule *Start work* and *Open in browser* follow:
          a disabled control claims there is something here to do.
        -->
        {#if canTransition && detail}
          <div>
            <div class="l">Status</div>
            <div class="v">
              <select
                class="sel-inline"
                aria-label="Change status"
                disabled={moving}
                value={currentStatus ?? ""}
                onchange={(event) => void move(event)}
              >
                <!--
                  Where the ticket stands, first and unselectable. It has to be
                  in the list for the control to show it at all, and it is not
                  somewhere the ticket can be moved *to*: "no status" is a place
                  a ticket can be and not one anything moves to, and a workflow
                  that offers the status a ticket is already in (Jira's
                  simplified workflow does) is offering a move that changes
                  nothing.
                -->
                {#if currentStatus === null}
                  <option value="" disabled>No status</option>
                {:else}
                  <option value={currentStatus} disabled>{currentStatus}</option>
                {/if}
                {#each offered as status (status)}
                  <option value={status}>{status}</option>
                {/each}
              </select>
              <!--
                The offer is the corpus one and nobody has checked it against
                the workflow (#498). Said rather than hidden: the select still
                works and the write still resolves, so what changes is only how
                much the reader should trust the list -- and a list that quietly
                went back to guessing is the thing this read was added to stop.
              -->
              {#if offerUnverified}
                <span class="unverified" title="the workflow could not be read; this is what this source's tickets have been seen to use">
                  offer unverified
                </span>
              {/if}
            </div>
          </div>
        {/if}
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

      {#if viewFailure !== null}
        <!-- One line, and only for a read that failed (ADR-0020 rule 2). -->
        <p class="mirror-line">
          <span class="pulse"></span>
          Showing the mirror: {viewFailure}
        </p>
      {:else if !viewAnswered && viewOverdue && !viewMemory.drawsNothing(detail.source.id, detail.row.kind)}
        <!-- Past the patience (#693): the mirror now, and the view when it lands. -->
        <p class="mirror-line">
          <span class="pulse"></span>
          Showing the mirror: {detail.source.display_name} has not answered yet
        </p>
      {/if}

      {#if sendingLines.length > 0}
        <!--
          Writes on their way that change nothing drawn above (#752): one line
          each, muted, until the queue settles them. A comment goes here only
          where there is no view to put it at the end of.
        -->
        <ul class="sending-lines" aria-label="Writes on their way">
          {#each sendingLines as line (line.key)}
            <li>
              <span class="sending-what">{line.text}</span>
              <span class="sending-mark">{line.mark}</span>
            </li>
          {/each}
        </ul>
      {/if}

      <!--
        The builds of the commit this stands at (#710), as one line (#732):
        between the view's toolbar and its tabs, where whether the code is
        green is asked, and under the header where there is no view (yet, or
        at all). The mirror's rows, drawn whatever the view did; absent where
        nothing ran on the commit. Its list stays open per entity, so the
        view landing does not close it.
      -->
      {#snippet buildsLine()}
        {#if detail}
          <BuildsSummary
            builds={detail.builds}
            onopen={onnavigate}
            bind:open={() => buildsOpenFor === detail?.row.entity_id, (open) => (buildsOpenFor = open ? (detail?.row.entity_id ?? null) : null)}
          />
        {/if}
      {/snippet}

      {#if sourceView}
        <SourceView
          summary={buildsLine}
          entityId={detail.row.entity_id}
          view={sourceView}
          webUrl={webUrl}
          sourceName={detail.source.display_name}
          writeOps={kindRegistry.writeOps(detail.source.adapter_kind)}
          {onnavigate}
          onopenurl={(url) => void open(url)}
          onaction={queueAction}
          {settingMarks}
          onsetting={saveSetting}
          said={saidOnTheirWay}
        />
      {:else}
        {@render buildsLine()}
      {/if}

      <!--
        A Confluence page renders its own markup; everything else renders the
        normalized text the mirror holds. Two arms of one section rather than
        two sections, because it is one thing — what this item says — and a
        panel that could show both would show a page twice.
      -->
      {#if viewWaiting}
        <!--
          The body's place while the view is read (#693): the height a view
          takes reserved, so nothing below moves when it lands, and one quiet
          line once the read has taken long enough to be news (#703). The
          header, path and links above and below are the mirror's and are
          already drawn.
        -->
        <div class="sec view-wait" aria-busy="true">
          {#if slow}
            <p class="muted">Reading from {detail.source.display_name}…</p>
          {/if}
        </div>
      {:else if pageParts}
        <div class="sec">
          <div class="sec-h"><span class="lab">Description</span></div>
          {#if pageParts.preamble.length > 0}
            <div class="d-body storage">
              <StorageBody nodes={pageParts.preamble} onopenlink={(href) => void open(href)} />
            </div>
          {/if}
          <!--
            One entry per section, so an *Edit* names the heading it belongs to
            (#286). A section that refuses itself says why and offers the wiki
            instead; a page nothing offers page writes for renders exactly as
            it did before this ticket, with no footer at all.
          -->
          {#each pageParts.sections as part (part.section.index)}
            <div class="pg-sec">
              {#if editingSection === part.section.index}
                <div class="pg-edit">
                  <span class="lab" id="pg-edit-label-{part.section.index}">
                    {part.section.heading}
                  </span>
                  <textarea
                    class="inp"
                    aria-labelledby="pg-edit-label-{part.section.index}"
                    bind:value={sectionDraft}
                    rows="8"
                  ></textarea>
                  {#if part.section.flattens}
                    <p class="pg-note">
                      This section has sub-headings, lists or formatting knobas rewrites as plain
                      paragraphs.
                    </p>
                  {/if}
                  <footer class="pg-acts">
                    <button
                      class="btn pri sm"
                      disabled={queueingSection}
                      onclick={() => void queueSectionEdit(part.section)}
                    >
                      Queue edit
                    </button>
                    <button class="btn ghost sm" onclick={() => (editingSection = null)}>
                      Cancel
                    </button>
                  </footer>
                </div>
              {:else}
                <div class="d-body storage">
                  <StorageBody nodes={part.nodes} onopenlink={(href) => void open(href)} />
                </div>
                {#if pageWritesOffered}
                  <!--
                    Three states, and the middle one is why `pageWritesOffered`
                    and `canEditPage` are two flags: a section that refuses
                    itself, a page knobas cannot tell the version of, and a
                    section that can be edited. Folding the first two into the
                    edit gate would take the *refusal* away with the offer, and
                    a section that silently offers nothing is worse than one
                    that says why.
                  -->
                  <footer class="pg-acts">
                    {#if part.section.refusal !== null}
                      <span class="pg-note">
                        {part.section.refusal === "macro"
                          ? "This section has a macro, so knobas will not rewrite it."
                          : "This section has a table, so knobas will not rewrite it."}
                      </span>
                      {#if webUrl}
                        <button class="btn sm" onclick={() => void open(webUrl)}>
                          Open in browser
                        </button>
                      {/if}
                    {:else if !canEditPage}
                      <span class="pg-note">
                        knobas cannot tell what version this page is at, so it will not rewrite
                        it.
                      </span>
                      {#if webUrl}
                        <button class="btn sm" onclick={() => void open(webUrl)}>
                          Open in browser
                        </button>
                      {/if}
                    {:else}
                      <button class="btn sm" onclick={() => editSection(part.section)}>
                        Edit section
                      </button>
                    {/if}
                  </footer>
                {/if}
              {/if}
            </div>
          {/each}
        </div>
      {:else if detail.body_text && !sourceView}
        <!--
          Not under a drawn view: the mirror's body text is what the source
          said about the item, and a view draws that itself (a commit's
          message is its overview's text), so a second copy would be noise.
        -->
        <div class="sec">
          <div class="sec-h"><span class="lab">Description</span></div>
          <p class="d-body">{detail.body_text}</p>
        </div>
      {/if}

      <!--
        Under the body, which is where a page's discussion belongs and where
        Confluence itself puts it. Absent rather than empty when there are
        none: a *Comments* heading over nothing is a section that says only
        that the reader has been counted.
      -->
      {#if !viewWaiting && (pageComments.length > 0 || canCommentOnPage)}
        <div class="sec">
          <div class="sec-h">
            <span class="lab">Comments</span>
            <span class="k muted">{pageComments.length}</span>
          </div>
          {#each pageComments as comment (comment.id)}
            <div class="cmt">
              <!--
                Who wrote it and when, where the record says (#286). Absent
                rather than "—" per field: a byline that reads "unknown" over
                somebody's words says less than no byline at all.
              -->
              {#if comment.author || comment.when}
                <div class="cmt-by k muted">
                  {#if comment.author}<span>{comment.author}</span>{/if}
                  {#if comment.when}<span>{ago(comment.when)}</span>{/if}
                </div>
              {/if}
              <div class="d-body storage">
                <StorageBody nodes={comment.nodes} onopenlink={(href) => void open(href)} />
              </div>
            </div>
          {/each}
          {#if canCommentOnPage}
            {#if commenting}
              <div class="pg-edit">
                <span class="lab" id="pg-comment-label">Your comment</span>
                <textarea
                  class="inp"
                  aria-labelledby="pg-comment-label"
                  bind:value={commentDraft}
                  rows="4"
                ></textarea>
                <footer class="pg-acts">
                  <button
                    class="btn pri sm"
                    disabled={queueingComment || commentDraft.trim() === ""}
                    onclick={() => void queueComment()}
                  >
                    Queue comment
                  </button>
                  <button class="btn ghost sm" onclick={() => (commenting = false)}>Cancel</button>
                </footer>
              </div>
            {:else}
              <footer class="pg-acts">
                <button class="btn sm" onclick={() => (commenting = true)}>Comment</button>
              </footer>
            {/if}
          {/if}
        </div>
      {/if}

      {#if sourceView}
        <!--
          The generic projection, kept and out of the way (story 13): nothing
          the mirror holds is lost, and none of it is in the reader's path.
          Drawn only while open -- forty rows of JSON nobody asked to see are
          not worth laying out.
        -->
        <details class="sec raw" ontoggle={(event) => (rawOpen = event.currentTarget.open)}>
          <summary class="sec-h">
            <span class="lab">Raw</span>
            <span class="k muted">{fields.length}</span>
          </summary>
          {#if rawOpen}
            <PayloadView {fields} />
          {/if}
        </details>
      {:else if !viewWaiting}
        <div class="sec">
          <div class="sec-h">
            <span class="lab">Details</span>
            <span class="k muted">{fields.length}</span>
          </div>
          <PayloadView {fields} />
        </div>
      {/if}

      <!--
        Where this repo's clone is on this disk (#499). Above the links because
        it is a fact about the item itself, and mounted only for the two kinds
        that have one -- an absent panel is the same honest rule *Start work*
        and *Open in browser* follow.
      -->
      {#if hasCheckout}
        <CheckoutPanel
          entityId={detail.row.entity_id}
          onread={(entityId, read) => (localTree = { entityId, tree: read?.tree ?? null })}
        />
      {/if}

      <LinksPanel
        entityId={detail.row.entity_id}
        links={detail.links}
        onopen={onnavigate}
        onunlink={(entry) => void removeLink(entry)}
        onlink={() => (linking = true)}
      />
      <HistoryPanel activity={detail.activity} />

      <!--
        Mounted only while it is open, and keyed on nothing: a closed dialog
        that keeps its half-typed search around is a dialog that reopens with
        somebody else's question in it.
      -->
      {#if linking}
        <LinkDialog
          fromId={detail.row.entity_id}
          fromTitle={detail.row.title}
          onclose={() => (linking = false)}
          oncreated={() => {
            linking = false;
            void refresh();
          }}
        />
      {/if}
    {/if}
  </div>
{#if resending}
  {@const again = resending}
  <!-- *Edit and resend* (#752): the dialog as it was sent, the reader's words in it. -->
  <ActionDialog
    action={again.action}
    start={again.values}
    startSettings={again.changed}
    settings={viewSettings}
    pending={sendsComments(again.action) ? pendingComments.of(again.entity) : []}
    onsubmit={(values, changed) => queueAction(again.action, values, changed, again.replaces)}
    onclose={() => (resending = null)}
  />
{/if}
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

  /*
    The view could not be read: one quiet line over the mirror's panel, the
    `.prompt` treatment without its band -- the panel below still works.
  */
  .mirror-line {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    border-bottom: 1px solid var(--hair);
    font: 400 11.5px var(--mono);
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .mirror-line .pulse {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--amber);
  }

  /*
    Writes on their way (#752). Colour only where something moves: the
    hairline and the mark are the link blue the settings rows' *saving…*
    uses; the words themselves step back to muted. A chip says where the
    entity will stand, a title its new name, a line anything else.
  */
  .sending-chip {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    margin-left: 8px;
    padding: 0 6px;
    border: 1px solid color-mix(in srgb, var(--link) 45%, transparent);
    border-radius: 3px;
    font: 500 11px/18px var(--mono);
    color: var(--muted);
  }

  .sending-mark {
    margin-left: 8px;
    font: 400 11px var(--mono);
    color: var(--link);
  }

  .sending-chip .sending-mark {
    margin-left: 0;
  }

  .sending-title {
    color: var(--muted);
  }

  .sending-lines {
    list-style: none;
    margin: 0;
    padding: 6px 14px;
    border-bottom: 1px solid var(--hair);
    font: 400 12px var(--sans);
    color: var(--muted);
  }

  .sending-lines li {
    display: flex;
    align-items: baseline;
    gap: 0;
    padding: 2px 0 2px 10px;
    border-left: 1px solid var(--link);
    overflow-wrap: anywhere;
  }

  .sending-lines li + li {
    margin-top: 4px;
  }

  .sending-what {
    min-width: 0;
  }

  /*
    The placeholder holds roughly the height a view's tab bar and first
    section take, so the links under it stay put when the tabs arrive.
  */
  .view-wait {
    min-height: 320px;
  }

  .view-wait p {
    font: 400 12px var(--mono);
  }

  /* *Raw*: a disclosure in the section's own heading, quiet until opened. */
  .raw > summary {
    cursor: pointer;
    list-style: none;
    margin-bottom: 0;
  }

  .raw > summary::-webkit-details-marker {
    display: none;
  }

  .raw > summary::before {
    content: "▸";
    color: var(--faint);
    font-size: 10px;
  }

  .raw[open] > summary {
    margin-bottom: 6px;
  }

  .raw[open] > summary::before {
    content: "▾";
  }

  .acts {
    margin-top: 12px;
    display: flex;
    gap: 6px;
  }

  /*
    The offer beside it is the corpus one, unchecked against the workflow
    (#498). Quiet on purpose: it qualifies a control that still works, so it
    reads as a footnote to the select rather than as a failure of the panel.
  */
  .unverified {
    margin-left: 6px;
    font: 500 11px var(--mono);
    color: var(--muted);
  }

  /*
    Body text is a source system's prose: newlines are meaningful, and one long
    unbroken token (a URL, a stack frame) must wrap rather than widen the panel.
  */
  .d-body {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  /*
    A rendered page body. `pre-wrap` is the plain-text arm's rule and would be
    wrong here -- the markup already says where the lines are, and preserving
    the storage format's own indentation would put a leading gap in front of
    every heading.

    **Why these rules are here, reaching into a child through `:global`,**
    rather than in `StorageBody.svelte` where the elements are made: Svelte
    scopes a component's CSS by stamping a class onto the elements its own
    selectors could match, and `StorageBody` emits every element through
    `<svelte:element this={...}>`, whose tag is not knowable at compile time.
    Giving that component an element selector therefore puts a `class`
    attribute on every node of a page body -- and *no attribute on any of them*
    is precisely the property `StorageBody.test.svelte.ts` asserts, one
    assertion at the centre of this ticket's gotcha-7 case. A styling
    convenience is not worth weakening that witness, so the page's typography
    lives with the panel that owns the section, and the component's own
    `<style>` keeps only the two class selectors it writes itself.
  */
  .d-body.storage {
    white-space: normal;
  }

  .d-body.storage :global(h1),
  .d-body.storage :global(h2),
  .d-body.storage :global(h3),
  .d-body.storage :global(h4),
  .d-body.storage :global(h5),
  .d-body.storage :global(h6) {
    margin: 12px 0 4px;
    font-family: var(--disp);
    font-size: 14px;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }

  .d-body.storage :global(p),
  .d-body.storage :global(ul),
  .d-body.storage :global(ol),
  .d-body.storage :global(blockquote) {
    margin: 6px 0;
  }

  .d-body.storage :global(ul),
  .d-body.storage :global(ol) {
    padding-left: 18px;
  }

  .d-body.storage :global(blockquote) {
    padding-left: 10px;
    border-left: 2px solid var(--hair2);
    color: var(--muted);
  }

  .d-body.storage :global(code) {
    font-family: var(--mono);
    font-size: 12px;
  }

  /*
    A page's code block, and the one place the storage format's own
    whitespace is the author's: it scrolls sideways rather than widening the
    panel, the rule every wide thing in this app follows.
  */
  .d-body.storage :global(pre) {
    margin: 6px 0;
    padding: 8px;
    overflow-x: auto;
    background: var(--raised);
    border-radius: 3px;
    font-family: var(--mono);
    font-size: 12px;
    white-space: pre;
  }

  /*
    A table is a table (criterion 2). Its own scroll container, because a wiki
    table is as wide as somebody made it and the panel is 550px.
  */
  .d-body.storage :global(table) {
    display: block;
    overflow-x: auto;
    width: max-content;
    max-width: 100%;
    margin: 8px 0;
    border-collapse: collapse;
  }

  .d-body.storage :global(th),
  .d-body.storage :global(td) {
    padding: 3px 8px;
    border: 1px solid var(--hair);
    text-align: left;
    vertical-align: top;
  }

  .d-body.storage :global(th) {
    background: var(--raised);
    font-weight: 600;
  }

  /* One comment, separated from the next by a hairline rather than a card. */
  .cmt {
    padding: 8px 0;
    border-top: 1px solid var(--hair);
  }

  .cmt:first-of-type {
    border-top: 0;
  }

  /* Who wrote a comment and when, above its words (#286). */
  .cmt-by {
    display: flex;
    gap: 8px;
    margin-bottom: 2px;
  }

  /*
    One section of a page, with whatever it offers under it. No border and no
    background: the sections are the page's own structure, and boxing each one
    would make a document look like a form.
  */
  .pg-sec + .pg-sec {
    margin-top: 4px;
  }

  .pg-acts {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 4px 0 8px;
  }

  .pg-edit {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 8px 0;
  }

  .pg-note {
    color: var(--muted);
    font-size: 12px;
    margin: 0;
  }

  /*
    Where the item lives, under its title: quieter than the title and allowed
    to wrap, because a path is prose of unbounded length and truncating it
    throws away the end — which is the half nearest the page.
  */
  .d-path {
    margin-top: 2px;
    color: var(--faint);
    font-size: 11.5px;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
</style>
