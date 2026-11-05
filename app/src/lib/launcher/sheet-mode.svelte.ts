/**
 * The Tab sheet as a mode of the box (#2426), its flows (#2427) and the bound
 * chords that run its actions (#2429): the state that reads and writes the
 * open sheet, the session, the box's focus and the ports together. What each
 * kind offers is the table in `sheet.ts`; this module carries it out.
 *
 * Construct it while a component initialises: it holds two effects, one that
 * closes a sheet whose row the selection has left and one that tells the key
 * panel when a key is let go. The host's getters are read when they are used,
 * so a prop that changes is seen.
 */
import { tick } from "svelte";

import { ipcErrorMessage } from "../ipc";
import type { LinkSetRow, ViewAction } from "../ipc/entity";
import { drawn } from "../detail/relations";
import { linkFailureMessage } from "../detail/links";
import { push } from "../shell/toasts.svelte";
import { JOB_LINK_RELATIONS } from "../tf/job/link-relations";
import { argvText, menuOrder } from "../tf/openin/openin";
import { KeyPanelMode, type KeyTarget } from "./key-panel.svelte";
import { type Chord, chordOf, isBoxChord, isHelpPress, refusalOf } from "./keys";
import type { KeyStore } from "./keys.svelte";
import type { LauncherRow } from "./rows";
import { walk } from "./rows";
import type { Session } from "./session.svelte";
import {
  type Choice,
  type SheetAction,
  type SheetEffect,
  type SheetSubject,
  type Step,
  favouriteKeys,
  kindOf,
  nameOf,
  narrowActions,
  sheetActions,
  subjectOf,
} from "./sheet";
import type { SheetPorts } from "./sheet-ports";

/**
 * A flow in the sheet: `id` tells one flow's late answer from another's,
 * `picks` are the steps answered, `asked` the choices each of them offered
 * (so a step back draws them again without a read), and `choices` the
 * current step's once read, `null` while reading.
 */
export interface Flow {
  id: number;
  steps: Step[];
  finish: (picks: readonly string[]) => SheetEffect;
  picks: string[];
  asked: Choice[][];
  choices: Choice[] | null;
}

/**
 * An open sheet. `row` is the row it lifted, `filter` what the box holds while
 * it is open (the search words wait in `session.raw`), `relation` the index
 * into `JOB_LINK_RELATIONS` the link row's chips stand on, `renaming` a saved
 * list's new name while the box asks for it, `flow` the flow an action started,
 * and `panel` the key panel on it.
 */
export interface OpenSheet {
  rowId: string;
  row: LauncherRow;
  subject: SheetSubject;
  filter: string;
  selected: number;
  relation: number;
  renaming: string | null;
  flow: Flow | null;
  panel: KeyPanelMode | null;
}

export interface SheetHost {
  session: Pick<Session, "current" | "home" | "error" | "set" | "renameList" | "deleteList" | "favourite">;
  io: SheetPorts;
  readonly keys: KeyStore;
  /** The job the shell names; *Link to ⟨label⟩* writes against it. */
  readonly job: { entityId: string; label: string } | undefined;
  /** *Start timer*'s way to the shell; `undefined` leaves the row out. */
  readonly ontimer: ((targetId: string, targetTitle: string) => void) | undefined;
  /** The clock a flow's dates are read against; the real one when `undefined`. */
  readonly now: Date | undefined;
  /** The source ops the backend offered on an inbox line. */
  inboxOps: (key: string) => readonly string[];
  /** Give the box its focus, its text selected when asked. */
  focus: (how?: { select?: boolean }) => void;
  /** Close the launcher. */
  close: () => void;
  /** Go to an address of the app. */
  navigate: (hash: string) => void;
}

/** ⌘B (Ctrl+B off the Mac), with no other modifier, by its character or its code. */
const isBindChord = (event: KeyboardEvent) => isBoxChord(event, "b", "KeyB");

export class SheetMode {
  /** The open sheet, or `null`. The markup reads it; every change goes through a method. */
  current = $state<OpenSheet | null>(null);

  readonly #host: SheetHost;

  /**
   * Settings › Checkouts, the default first: *Open in ⟨app⟩*'s rows. Read the
   * first time a sheet opens in an opening, so a search that acts on nothing
   * costs no read; the rows join the sheet when the answer lands.
   */
  #applications = $state<{ id: string; name: string }[]>([]);
  #applicationsRead = false;
  /** The applications' read in flight, which a chord on *Open in ⟨app⟩* waits for. */
  #applicationsReading: Promise<unknown> = Promise.resolve();
  /** Every link set: *Add to a set…*'s choices, read with the applications. */
  #linkSets = $state<LinkSetRow[]>([]);
  /** The declared merge of the pull request the sheet opened on, read once per sheet: *Merge…* follows it. */
  #merge = $state<{ prId: string; action: ViewAction | null } | null>(null);
  /** The merge's read in flight, which a chord on *Merge…* waits for. */
  #mergeReading: Promise<unknown> = Promise.resolve();
  #flows = 0;

  /** What is a favourite now (#2428): the board's block, so the sheet offers *Unfavourite* on it. */
  #favourites = $derived.by(() => favouriteKeys(this.#host.session.home?.favourites ?? []));
  #context = $derived.by(() => ({
    job: this.#host.job ?? null,
    canTime: this.#host.ontimer !== undefined,
    apps: this.#applications,
    favourites: this.#favourites,
    sets: this.#linkSets,
    merge: this.#mergeOf(this.current?.subject),
    branches: this.#host.io.branches,
    now: this.#host.now ?? new Date(),
  }));

  /** The actions as the box narrows them; none while a flow is open. */
  readonly shown = $derived(this.current && !this.current.flow ? narrowActions(sheetActions(this.current.subject, this.#context), this.current.filter) : []);
  /** The flow's current step's choices, as the box narrows them. */
  readonly choices = $derived(this.current?.flow ? narrowActions(this.current.flow.choices ?? [], this.current.filter) : []);
  /** The flow's current step. */
  readonly step = $derived(this.current?.flow ? this.current.flow.steps[this.current.flow.picks.length] : undefined);
  /** The index of the row the box names as active, or `null` when it names none. */
  readonly active = $derived.by(() => {
    const open = this.current;
    if (!open || open.renaming !== null) return null;
    return (open.flow ? this.choices.length : this.shown.length) > 0 ? open.selected : null;
  });
  /** Whether the actions are what the box narrows, so ⌘B has one to bind. */
  readonly onActions = $derived(!!this.current && !this.current.flow && this.current.renaming === null && !this.current.panel);
  /** The label in front of the box while a sheet is open. */
  readonly label = $derived(
    this.current?.panel ? "Keys" : this.current?.renaming != null ? "New name" : this.current?.flow ? (this.step?.title ?? "Pick") : "Actions",
  );
  /** What the empty box says while a sheet is open. */
  readonly placeholder = $derived(
    this.current?.panel
      ? "press the keys"
      : this.current?.renaming != null
        ? "the list's new name"
        : this.current?.flow
          ? "narrow the choices"
          : "narrow the actions",
  );
  /** What the box holds while a sheet is open. */
  readonly text = $derived(this.current ? (this.current.panel ? "" : (this.current.renaming ?? this.current.filter)) : "");

  constructor(host: SheetHost) {
    this.#host = host;
    // The sheet belongs to one row: a new answer that moves the selection off it closes it.
    $effect(() => {
      if (this.current && host.session.current?.id !== this.current.rowId) this.current = null;
    });
    // The panel draws the modifiers as they are let go, too.
    $effect(() => {
      const panel = this.current?.panel;
      if (!panel) return;
      const up = (event: KeyboardEvent) => panel.release(event);
      window.addEventListener("keyup", up);
      return () => window.removeEventListener("keyup", up);
    });
  }

  /** Whether Tab on `row` has something to open. */
  opensOn(row: LauncherRow | null): boolean {
    const subject = row ? subjectOf(row, this.#host.inboxOps) : null;
    return subject !== null && sheetActions(subject, this.#context).length > 0;
  }

  /** Tab on the selected row: the sheet, the box emptied and labelled Actions. A row with no actions opens nothing. */
  open(): void {
    const row = this.#host.session.current;
    const subject = row ? subjectOf(row, this.#host.inboxOps) : null;
    if (!row || !subject || sheetActions(subject, this.#context).length === 0) return;
    this.current = sheetOn(row, subject);
    this.#readApplications();
    this.#readMerge(subject);
    this.#host.focus();
  }

  /** Back to the results, the search words in the box as they were. */
  leave(): void {
    this.current = null;
    this.#host.focus();
  }

  /** The launcher closed: the next opening starts with nothing open and reads again. */
  reset(): void {
    this.current = null;
    this.#applicationsRead = false;
    this.#merge = null;
  }

  /** Typing in the box while a sheet is open: the new name, or the filter. The panel takes no text. */
  type(value: string): void {
    const open = this.current;
    if (!open || open.panel) return;
    if (open.renaming !== null) {
      open.renaming = value;
      return;
    }
    open.filter = value;
    open.selected = 0;
  }

  /** The pointer on row `index` selects it. */
  hover(index: number): void {
    if (this.current) this.current.selected = index;
  }

  /** The link row's relation chip `index` chosen. */
  relate(index: number): void {
    if (this.current) this.current.relation = index;
  }

  /** Carry out one action of the open sheet. */
  perform(action: SheetAction): void {
    this.#carry(action.does);
  }

  /** The chord drawn on an action row: the list's own on *Assign key…*, else the action's on the subject's kind. */
  boundTo(action: SheetAction): Chord | null {
    const subject = this.current?.subject;
    if (!subject) return null;
    const { keys } = this.#host;
    if (action.does.kind === "bind" && subject.kind === "list") return keys.chordOfList(subject.id);
    return keys.chordOfAction(kindOf(subject), action.id);
  }

  /**
   * A keydown while a sheet is open, from the actions, a flow or a rename. The
   * key panel takes its own first (`current.panel.press`), because its chords
   * are what is being bound; the caller asks that before anything else.
   */
  key(event: KeyboardEvent): void {
    const open = this.current;
    if (!open) return;
    // The box's own chords are not the sheet's: ⌘S and ⌘? do nothing here.
    if (isBoxChord(event, "s") || isHelpPress(event)) {
      event.preventDefault();
      return;
    }
    if (isBindChord(event)) {
      event.preventDefault();
      this.#openKeyPanel(open);
      return;
    }
    if (open.renaming !== null) return this.#renamingKey(event, open, open.renaming);
    if (open.flow) return this.#flowKey(event, open, open.flow);
    const to = walk(event, open.selected, this.shown.length);
    if (to !== null) {
      open.selected = to;
      return;
    }
    switch (event.key) {
      case "ArrowLeft":
      case "ArrowRight":
        if (this.shown[open.selected]?.does.kind === "link") {
          event.preventDefault();
          const step = event.key === "ArrowRight" ? 1 : -1;
          open.relation = (open.relation + step + JOB_LINK_RELATIONS.length) % JOB_LINK_RELATIONS.length;
        }
        return;
      case "Enter": {
        event.preventDefault();
        const chosen = this.shown[open.selected];
        if (chosen) this.perform(chosen);
        return;
      }
      case "Tab":
        event.preventDefault();
        this.leave();
        return;
      case "Escape":
        // Never the dialog's: the sheet goes back to the results.
        event.preventDefault();
        event.stopPropagation();
        this.leave();
        return;
      case "Backspace":
        if (open.filter === "") {
          event.preventDefault();
          this.leave();
        }
        return;
      default:
    }
  }

  /**
   * A bound chord, from the results or the sheet's actions: a list's applies
   * it, an action's runs on the selected row when it is bound on that row's
   * kind. `true` when the chord was one; every other key goes on as before.
   * Never in a flow or a rename, whose keys are their own.
   */
  runChord(event: KeyboardEvent): boolean {
    if (this.current && (this.current.flow || this.current.renaming !== null)) return false;
    if (refusalOf(event) !== null) return false;
    const chord = chordOf(event);
    if (chord === null) return false;
    const { keys, session, inboxOps } = this.#host;
    const list = keys.listOf(chord);
    if (list) {
      event.preventDefault();
      this.current = null;
      session.set(`${list.text} `);
      this.#host.focus();
      return true;
    }
    const row = this.current?.row ?? session.current;
    const subject = row ? subjectOf(row, inboxOps) : null;
    if (!row || !subject) return false;
    const id = keys.actionOf(kindOf(subject), chord);
    if (id === null) return false;
    event.preventDefault();
    void this.#runAction(row, subject, id, chord);
    return true;
  }

  /** Take `choice` for the flow's current step: the next step's choices, or the last pick runs the flow's finish. */
  pick(choice: Choice): void {
    const open = this.current;
    const flow = open?.flow;
    if (!open || !flow) return;
    const picks = [...flow.picks, choice.id];
    if (picks.length >= flow.steps.length) {
      const finished = flow.finish(picks);
      open.flow = null;
      open.filter = "";
      open.selected = 0;
      this.#carry(finished);
      return;
    }
    flow.asked = [...flow.asked, flow.choices ?? []];
    flow.picks = picks;
    flow.choices = null;
    open.filter = "";
    open.selected = 0;
    void this.#readStep(flow.id, picks.length);
  }

  /**
   * Carry out what an action, or a flow's last pick, does: the one reader of
   * `SheetEffect`. On the open sheet's row, or on `at` when a chord runs an
   * action that needs no sheet (#2429).
   */
  #carry(does: SheetEffect, at: { subject: SheetSubject; relation: number } | null = this.current): void {
    if (!at) return;
    const { subject, relation } = at;
    const { io, session, focus, close, navigate } = this.#host;
    const settled = (work: () => Promise<unknown>, said: () => string) => void this.#settled(work, said);
    switch (does.kind) {
      case "go":
        close();
        return navigate(does.hash);
      case "copy":
        return settled(() => io.copyText(does.text), () => does.said);
      case "write":
        return settled(() => io.submitWrite(does.payload), () => does.said);
      case "ack":
        return settled(() => io.ackAlert(does.monitorId), () => does.said);
      case "link": {
        const job = this.#host.job;
        if (!job || subject.kind !== "entity") return;
        const picked = JOB_LINK_RELATIONS[relation] ?? JOB_LINK_RELATIONS[0]!;
        // The job's own mapping (`TfLinkTo`): drawn from the job to the row.
        const row = drawn(picked.id, job.entityId, subject.entityId);
        void io.createLink(row.fromId, row.toId, row.relation).then(
          () => {
            push({ text: `Linked ${nameOf(subject)} to ${job.label} as ${picked.label}` });
            close();
          },
          (cause: unknown) => {
            push({ text: linkFailureMessage(cause), tone: "err" });
            focus();
          },
        );
        return;
      }
      case "timer": {
        const { ontimer } = this.#host;
        if (subject.kind !== "entity" || !ontimer) return;
        ontimer(subject.entityId, subject.title);
        return close();
      }
      case "open-in": {
        if (subject.kind !== "entity") return;
        let argv: string[] = [];
        return settled(
          async () => {
            const where = subject.entityKind === "pr" ? await io.repoOf({ sourceId: subject.sourceId, path: subject.path }) : subject.entityId;
            if (where === null) throw new Error(`The mirror holds no repository for ${subject.title}.`);
            argv = await io.openCheckout(where, { application: does.application });
          },
          () => `Started ${does.name}: ${argvText(argv)}`,
        );
      }
      case "clone":
        if (subject.kind !== "entity") return;
        return settled(() => io.clone({ entityId: subject.entityId, title: subject.title }), () => `Cloning ${subject.title}`);
      case "apply":
        if (subject.kind !== "list") return;
        this.current = null;
        // As Enter on the list does (#2423): its text back in the box, or why it cannot be.
        if (subject.refusal !== null) push({ text: subject.refusal });
        else session.set(`${subject.text} `);
        focus();
        return;
      case "rename":
        if (subject.kind !== "list" || !this.current) return;
        this.current.renaming = subject.label;
        void tick().then(() => focus({ select: true }));
        return;
      case "delete":
        if (subject.kind !== "list") return;
        this.current = null;
        focus();
        void session.deleteList(subject.id).then((done) => done && push({ text: `Deleted ${subject.label}` }));
        return;
      case "favourite": {
        // Back to the results or the board, which redraws with the block as
        // the backend now has it; the launcher stays open.
        this.current = null;
        focus();
        const name = nameOf(subject);
        void session.favourite(does.target, does.on).then((done) => {
          if (done) push({ text: `${does.on ? "Favourited" : "Unfavourited"} ${name}` });
          else push({ text: session.error ?? "The favourite was not kept.", tone: "err" });
        });
        return;
      }
      case "flow":
        return this.#startFlow(does.steps, does.finish);
      case "bind":
        // *Assign key…*: the panel, on the chord the list has now.
        if (subject.kind !== "list" || !this.current) return;
        this.#panelOn(this.current, { kind: "list", id: subject.id, label: subject.label });
        return;
      case "save-set":
        return settled(() => io.saveLinkSet(does.draft), () => does.said);
      case "snooze":
        return settled(() => io.inboxSnooze(does.key, does.until), () => does.said);
      case "refused":
        push({ text: does.said });
        focus();
        return;
      case "inbox": {
        if (subject.kind !== "inbox") return;
        const { item } = subject;
        if (does.op === "rerun") {
          return settled(() => io.submitWrite({ RerunBuild: { entity: item.entity_id ?? "" } }), () => `Re-run queued for ${item.title}`);
        }
        if (does.op === "ack") {
          return settled(() => io.inboxAck(item.key), () => "Acked. The alert stays open until the monitor recovers.");
        }
        return settled(() => io.inboxDone(item.key), () => `Done: ${item.title}`);
      }
    }
  }

  /** A write or a read that answers: said in a toast and the launcher closed, or the reason said and the sheet kept. */
  async #settled(work: () => Promise<unknown>, said: () => string): Promise<void> {
    try {
      await work();
      push({ text: said() });
      this.#host.close();
    } catch (cause) {
      push({ text: ipcErrorMessage(cause), tone: "err" });
      this.#host.focus();
    }
  }

  /** Start a flow in the sheet: the box empties, and the first step's choices are read. */
  #startFlow(steps: Step[], finish: Flow["finish"]): void {
    const open = this.current;
    if (!open || steps.length === 0) return;
    const flow: Flow = { id: ++this.#flows, steps, finish, picks: [], asked: [], choices: null };
    open.flow = flow;
    open.filter = "";
    open.selected = 0;
    void this.#readStep(flow.id, 0);
    this.#host.focus();
  }

  /** Read step `at` of flow `id`: an answer for a flow or a step no longer open is dropped. */
  async #readStep(id: number, at: number): Promise<void> {
    const flow = this.current?.flow;
    const step = flow?.steps[at];
    if (!flow || !step) return;
    const picks = [...flow.picks];
    // Still the step asked: the same flow, and the same picks (a step back and another pick reads it again).
    const asked = () => {
      const live = this.current?.flow;
      return live && live.id === id && live.choices === null && live.picks.length === picks.length && live.picks.every((one, i) => one === picks[i]) ? live : null;
    };
    try {
      const choices = await step.choices(picks);
      const live = asked();
      if (live) live.choices = choices;
    } catch (cause) {
      const open = this.current;
      if (!open || !asked()) return;
      push({ text: ipcErrorMessage(cause), tone: "err" });
      open.flow = null;
      open.filter = "";
      open.selected = 0;
    }
  }

  /** One step back: the step before, its choices as they were, or from the first step the actions. */
  #stepBack(open: OpenSheet, flow: Flow): void {
    open.filter = "";
    open.selected = 0;
    if (flow.picks.length === 0) {
      open.flow = null;
    } else {
      flow.picks = flow.picks.slice(0, -1);
      flow.choices = flow.asked.at(-1) ?? null;
      flow.asked = flow.asked.slice(0, -1);
      if (flow.choices === null) void this.#readStep(flow.id, flow.picks.length);
    }
    this.#host.focus();
  }

  /** A flow's keys: the arrows walk the choices, Enter picks, Esc or Backspace on the empty box step back, Tab leaves. */
  #flowKey(event: KeyboardEvent, open: OpenSheet, flow: Flow): void {
    const to = walk(event, open.selected, this.choices.length);
    if (to !== null) {
      open.selected = to;
      return;
    }
    switch (event.key) {
      case "Enter": {
        event.preventDefault();
        const chosen = this.choices[open.selected];
        if (chosen) this.pick(chosen);
        return;
      }
      case "Tab":
        event.preventDefault();
        this.leave();
        return;
      case "Escape":
        // Never the dialog's: a step back.
        event.preventDefault();
        event.stopPropagation();
        this.#stepBack(open, flow);
        return;
      case "Backspace":
        if (open.filter === "") {
          event.preventDefault();
          this.#stepBack(open, flow);
        }
        return;
      default:
    }
  }

  /** The new name's keys: Enter keeps it, Esc goes back to the list's actions. */
  #renamingKey(event: KeyboardEvent, open: OpenSheet, name: string): void {
    const { session, focus } = this.#host;
    if (event.key === "Enter") {
      event.preventDefault();
      const label = name.trim();
      if (label === "" || open.subject.kind !== "list") return;
      const { id } = open.subject;
      this.current = null;
      focus();
      void session.renameList(id, label).then((done) => done && push({ text: `Renamed to ${label}` }));
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      open.renaming = null;
      open.filter = "";
      focus();
    }
  }

  /** ⌘B on a selected action: the panel, on the chord the action has on this kind now. */
  #openKeyPanel(open: OpenSheet): void {
    const action = this.shown[open.selected];
    if (!action || open.flow || open.renaming !== null) return;
    // ⌘B on *Assign key…* is that row's own panel.
    if (action.does.kind === "bind") return this.#carry(action.does);
    this.#panelOn(open, { kind: "action", on: kindOf(open.subject), actionId: action.id, label: action.label });
  }

  #panelOn(open: OpenSheet, target: KeyTarget): void {
    const { keys, focus } = this.#host;
    open.panel = new KeyPanelMode(target, {
      keys,
      focus: () => focus(),
      leave: () => {
        if (this.current) this.current.panel = null;
      },
    });
    focus();
  }

  /**
   * Run action `id` on `row`. An action the reads add (*Open in ⟨app⟩*,
   * *Merge…*) waits for them. A flow, a rename, the panel and the link need
   * the sheet under them, so it opens on the row first; the rest run as
   * Enter on the row would, the sheet never drawn.
   */
  async #runAction(row: LauncherRow, subject: SheetSubject, id: string, chord: Chord): Promise<void> {
    this.#readApplications();
    this.#readMerge(subject);
    const find = () => sheetActions(subject, { ...this.#context, merge: this.#mergeOf(subject) }).find((action) => action.id === id);
    let action = find();
    if (!action) {
      await Promise.all([this.#applicationsReading, this.#mergeReading]);
      // The row the chord was pressed on is no longer the one selected: nothing runs.
      if ((this.current?.row ?? this.#host.session.current)?.id !== row.id) return;
      action = find();
    }
    if (!action) {
      push({ text: `${chord} runs nothing on ${nameOf(subject)} now` });
      return;
    }
    if (!["flow", "rename", "bind", "link"].includes(action.does.kind)) return this.#carry(action.does, { subject, relation: 0 });
    if (!this.current || this.current.rowId !== row.id) {
      this.current = sheetOn(row, subject);
    }
    if (action.does.kind === "link") {
      // The relation is picked on the row with ← →; Enter writes it.
      this.current.filter = "";
      this.current.selected = Math.max(0, this.shown.findIndex((shown) => shown.id === id));
      this.#host.focus();
      return;
    }
    this.#carry(action.does);
  }

  #readApplications(): void {
    if (this.#applicationsRead) return;
    this.#applicationsRead = true;
    const { io } = this.#host;
    void io.linkSets().then(
      (sets) => (this.#linkSets = sets),
      () => (this.#linkSets = []),
    );
    this.#applicationsReading = io.checkoutSettings().then(
      (settings) => (this.#applications = menuOrder(settings).map(({ app }) => ({ id: app.id, name: app.name }))),
      () => (this.#applications = []),
    );
  }

  #readMerge(subject: SheetSubject): void {
    if (subject.kind !== "entity" || subject.entityKind !== "pr") return;
    const prId = subject.entityId;
    if (this.#merge?.prId === prId) return;
    this.#merge = { prId, action: null };
    this.#mergeReading = this.#host.io.mergeAction(prId).then(
      (action) => {
        if (this.#merge?.prId === prId) this.#merge = { prId, action };
      },
      () => {},
    );
  }

  /** The declared merge read for `subject`, when it is the pull request read: *Merge…*'s source. */
  #mergeOf(subject: SheetSubject | undefined): ViewAction | null {
    return subject?.kind === "entity" && this.#merge?.prId === subject.entityId ? this.#merge.action : null;
  }
}

/** A fresh sheet on `row`: the actions, unnarrowed, the first selected. */
function sheetOn(row: LauncherRow, subject: SheetSubject): OpenSheet {
  return { rowId: row.id, row, subject, filter: "", selected: 0, relation: 0, renaming: null, flow: null, panel: null };
}
