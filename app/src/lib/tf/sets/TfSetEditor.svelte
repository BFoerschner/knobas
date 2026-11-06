<!--
  The set editor (#866, R-7.7): `#/sets/<id>`, prototype-6's `renderSet(id)`.

  - The name and the description, edited in place, and *Delete*.
  - One panel per kind (repositories, pipelines, pages, assets, monitors),
    each with its patterns ("Pattern: … (N match now)", expanding to the
    list) and its members (a dot and a line beside each name, #1008),
    *Remove* on each, *Add…* (`TfSetPicker.svelte`), and for pipelines and
    pages *Add a pattern…* (`TfSetPattern.svelte`, #868).
  - *Applies to*: one row per rule with a kind dropdown, a value dropdown, a
    live ticket count (`preview_link_set_rule`) and *Remove*, plus *Add a
    rule*. A rule waiting for its value says so, takes focus on its value and
    closes first on `Esc`, back to what opened it (`inlineEditor`).
  - *Shows on N tickets now*: how many are yours, and the keys as links to
    their jobs (yours in yellow), the first 40 then "and N more".

  Every change saves the whole set (`save_link_set`) and draws what the save
  answered, as prototype-6 has no Save button: a set is live, and every job it
  applies to reads the change on its next read. A *Remove* acts at once, so it
  toasts with *Undo*, and removals in a row collapse into one toast (the
  removal rule). Focus moves to the row that took its place. *Add…* and *Add a
  pattern…* toast with *Undo* too, and each *Undo* goes by the set's id (`undo-on.ts`).

  At `NEW_SET` nothing is saved yet: the first change makes the set, and the
  address moves to it in place. Leaving before that leaves no row behind.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { MonitorRow } from "../../ipc/assets";
  import type {
    LinkSetMemberRow,
    LinkSetDetail,
    LinkSetDraft,
    LinkSetMemberKind,
    LinkSetRule,
    LinkSetRuleKind,
    LinkSetRuleRow,
  } from "../../ipc/entity";
  import { push, tally, undoable } from "../../shell/toasts.svelte";
  import { keyOf } from "../job/job";
  import { entityAddress } from "../address";
  import { inlineEditor } from "../inline-editor";
  import { replaceAddress } from "../opener";
  import { holdStillPress } from "../still-press";
  import type { EscTarget } from "../escape";
  import TfDialog from "../settings/TfDialog.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfCrumbs from "../TfCrumbs.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";
  import { draftOf } from "./apply";
  import { editorShows, undoOn } from "./undo-on";
  import type { Candidate, RuleChoices, SetPorts } from "./ports";
  import { counted } from "../counted";
  import { assetLine, blankSet, itemsOf, KIND_LABEL, MEMBER_KINDS, NEW_SET, NEW_SET_NAME, onePerKey, PATTERN_KINDS, patternShown, putBack, RULE_KINDS, SHOWN_KEYS, type Removed } from "./sets";
  import { toneClass } from "../tone";
  import { kindWord, stateChip } from "../monitors/monitors";
  import { stateWord } from "../state-word";
  import TfSetPattern from "./TfSetPattern.svelte";
  import TfSetPicker from "./TfSetPicker.svelte";

  let {
    ready,
    id,
    ports,
    back = null,
  }: {
    ready: boolean;
    /** The set's id, from `#/sets/<id>`. */
    id: string;
    ports: SetPorts;
    /** Where `Esc` goes, which the breadcrumb's hint names. */
    back?: Pick<EscTarget, "name"> | null;
  } = $props();

  let set = $state<LinkSetDetail | null>(null);
  let failed = $state<string | null>(null);
  let missing = $state(false);
  /** Moves on *Try again*, which the read below follows. */
  let attempt = $state(0);
  /** Whether *Delete*'s confirmation is open. */
  let deleting = $state(false);
  let choices = $state<RuleChoices | null>(null);
  /** Live ticket counts per rule, by `kind|value`. */
  let counts = $state<Record<string, number>>({});
  /** A rule being added or re-kinded, until its value is chosen. */
  let pending = $state<{ at: number; kind: LinkSetRuleKind } | null>(null);
  let picking = $state<LinkSetMemberKind | null>(null);
  /** The kind whose *Add a pattern…* dialog is open. */
  let patterning = $state<LinkSetMemberKind | null>(null);
  /** What a member's line and a monitor's dot read from. Both can fail: the row then says what the set read said. */
  let typeNames = $state<Record<string, string>>({});
  let roster = $state<MonitorRow[]>([]);
  const memberLine = (member: LinkSetMemberRow) =>
    member.line === null ? null : member.kind === "asset" ? assetLine(member.line, typeNames) : member.kind === "monitor" ? kindWord(member.line) : member.line;
  /** A monitor's dot says what its chip on Monitors says, so a paused monitor is grey here too. */
  function memberState(member: LinkSetMemberRow) {
    const row = member.kind === "monitor" ? roster.find((held) => held.entity_id === member.entity_id) : undefined;
    if (row === undefined) return member.state;
    const chip = stateChip(row);
    return { text: chip.label, tone: chip.tone ?? ("muted" as const) };
  }
  /** The saves asked for so far, in order (see `save`). */
  let saves: Promise<void> = Promise.resolve();

  $effect(() => {
    void attempt;
    if (!ready) return;
    const wanted = id;
    let live = true;
    failed = null;
    missing = false;
    if (wanted === NEW_SET) {
      set = blankSet();
    } else if (wanted !== untrack(() => set?.id)) {
      // A set this editor has just made is already drawn from what its save answered: a read here could only land late.
      ports.linkSet(wanted).then(
        (detail) => {
          if (live) set = detail;
        },
        (error: unknown) => {
          if (!live) return;
          if ((error as { code?: string } | null)?.code === "not_found") missing = true;
          else failed = ipcErrorMessage(error);
        },
      );
    }
    ports.ruleChoices().then(
      (read) => {
        if (live) choices = read;
      },
      () => {},
    );
    ports.assetTypeNames().then(
      (read) => {
        if (live) typeNames = read;
      },
      () => {},
    );
    ports.monitors().then(
      (read) => {
        if (live) roster = read;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

  const countKey = (rule: Pick<LinkSetRule, "kind" | "value">) => `${rule.kind}|${rule.value}`;

  // Each rule's count is its own read, asked once per rule the set holds.
  $effect(() => {
    for (const rule of set?.rules ?? []) {
      const at = countKey(rule);
      if (at in counts) continue;
      counts[at] = -1;
      ports.previewLinkSetRule({ kind: rule.kind, value: rule.value }).then(
        (tickets) => (counts[at] = tickets.length),
        () => delete counts[at],
      );
    }
  });

  /**
   * Save the set as `change` leaves it, and draw what the save answered.
   * Saves run one after another, each over what the one before answered, so
   * a change made while a save is in flight is kept, not dropped.
   */
  function save(change: (draft: LinkSetDraft) => void, toast?: string, saved?: () => void): Promise<void> {
    const target = set?.id;
    saves = saves.then(async () => {
      // A change asked for before the first save answered goes onto the set that save made.
      if (!set || (target !== "" && set.id !== target)) return;
      const draft = draftOf(set);
      if (set.id === "") draft.id = null;
      change(draft);
      try {
        const answer = await ports.saveLinkSet(draft);
        if (set.id === "") {
          titleFor = answer.id;
          set = answer;
          replaceAddress(`#/sets/${encodeURIComponent(answer.id)}`);
        } else {
          set = answer;
        }
        if (toast) push({ text: toast });
        saved?.();
      } catch (error) {
        push({ text: ipcErrorMessage(error), tone: "err" });
      }
    });
    return saves;
  }

  /** This set is on screen, so a toast's *Undo* saves through this editor and redraws it. */
  $effect(() => {
    const shown = set?.id;
    if (!shown) return;
    return editorShows(shown, (change) => save(change));
  });

  function rename(event: Event, field: "name" | "description") {
    const el = event.currentTarget as HTMLElement;
    const text = el.textContent.trim();
    if (!set) return;
    // What the field draws is what the set holds, whitespace and a refused empty name gone.
    if (text === set[field] || (field === "name" && text === "")) {
      el.textContent = set[field];
      return;
    }
    void save((draft) => (draft[field] = text)).then(() => {
      if (document.activeElement !== el && set) el.textContent = set[field];
    });
  }

  /**
   * A field edited in place draws what the set holds, and is not redrawn: redrawing the header on a save took focus from
   * the field the reader had just tabbed to. While one has focus its words are the reader's.
   */
  function holds(read: () => string) {
    return (el: HTMLElement) => {
      const words = read();
      if (document.activeElement !== el && el.textContent !== words) el.textContent = words;
    };
  }

  let title = $state<HTMLElement | null>(null);
  /** The set the title last took focus for: its first save moves the address and is no new arrival. */
  let titleFor: string | null = null;

  /** A set with the name the button gave it: its name is selected on arrival, to type over. */
  $effect(() => {
    const el = title;
    const current = set;
    if (el === null || current === null || titleFor === current.id) return;
    titleFor = current.id;
    if (current.name !== NEW_SET_NAME) return;
    el.focus();
    const range = document.createRange();
    range.selectNodeContents(el);
    const selection = getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  });

  /**
   * A field edited in place keeps focus (F-539). `Enter` saves it. `Esc` on changed words puts the old ones back and the field
   * keeps the press; `Esc` on words nothing changed is no edit to undo, so it goes on to the shell and leaves the page, in one press.
   */
  function fieldKey(event: KeyboardEvent, field: "name" | "description") {
    const el = event.currentTarget as HTMLElement;
    if (event.key === "Escape" && set) {
      if (el.textContent.trim() === set[field]) {
        el.blur();
        return;
      }
      el.textContent = set[field];
      event.stopPropagation();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      rename(event, field);
    }
  }

  async function remove() {
    if (!set) return;
    deleting = false;
    const gone = set.id;
    const name = set.name;
    await saves;
    try {
      await ports.deleteLinkSet(gone);
      push({ text: `Deleted ${name}. Links drawn on tickets by hand stay.` });
      location.hash = "#/sets";
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    }
  }

  /** *Add* acts at once, so its toast takes it back, as a *Remove*'s does (F-547). It reads the set the save answered: a new set's id is only then known. */
  function addedToast(text: (name: string) => string, undo: (draft: LinkSetDraft) => void) {
    if (!set) return;
    const { id, name } = set;
    undoable(text(name), () => undoOn(id, undo, ports));
  }

  function addedPattern(kind: LinkSetMemberKind, query: string) {
    patterning = null;
    holdStillPress();
    void save((draft) => draft.patterns.push({ kind, query }), undefined, () =>
      addedToast(
        (name) => `Added a pattern to ${name}`,
        (draft) => {
          draft.patterns = draft.patterns.filter((held) => !(held.kind === kind && held.query === query));
        },
      ),
    );
  }

  function added(kind: LinkSetMemberKind, picked: Candidate[]) {
    picking = null;
    holdStillPress();
    const ids = new Set(picked.map((row) => row.entity_id));
    const what = picked.length === 1 ? picked[0]!.name : picked.length;
    void save((draft) => draft.members.push(...picked.map((row) => ({ kind, entity_id: row.entity_id }))), undefined, () =>
      addedToast(
        (name) => `Added ${what} to ${name}`,
        (draft) => {
          draft.members = draft.members.filter((held) => !ids.has(held.entity_id));
        },
      ),
    );
  }

  /** What a rule's value dropdown offers, the rule's own value always among them. */
  function options(kind: LinkSetRuleKind, current: string): { value: string; label: string }[] {
    const read = choices;
    const ticket = (row: { entity_id: string; title: string }) => ({
      value: row.entity_id,
      label: row.title && row.title !== keyOf(row.entity_id) ? `${keyOf(row.entity_id)} ${row.title}` : keyOf(row.entity_id),
    });
    let list: { value: string; label: string }[] = [];
    if (read) {
      if (kind === "project") {
        list = read.projects.map((project) => ({ value: `${project.source_id}:${project.key}`, label: project.key }));
      } else if (kind === "manual") {
        list = read.tickets.map(ticket);
      } else {
        const parents = new Set(read.parents);
        list = read.tickets.filter((row) => parents.has(keyOf(row.entity_id))).map(ticket);
      }
    }
    if (current && !list.some((option) => option.value === current)) list.unshift({ value: current, label: keyOf(current) });
    return list;
  }

  function rekind(at: number, kind: LinkSetRuleKind) {
    if (set?.rules[at]?.kind === kind) {
      // Back to what it holds: nothing is waiting any more.
      if (pending?.at === at) pending = null;
      return;
    }
    pending = { at, kind };
  }

  /** The row stays through the save, so a value picked keeps its place and focus stays on it. */
  function choose(at: number, kind: LinkSetRuleKind, value: string) {
    if (!value) return;
    const replacing = pending?.at === at && at < (set?.rules.length ?? 0);
    settled = true;
    void save((draft) => {
      const rule = { kind, value };
      if (draft.rules.some((held) => held.kind === kind && held.value === value)) {
        if (replacing) draft.rules.splice(at, 1);
        return;
      }
      if (at < draft.rules.length) draft.rules.splice(at, 1, rule);
      else draft.rules.push(rule);
    }).then(() => {
      if (pending?.at === at) pending = null;
    });
  }

  /** A new rule opens on the first kind with something to choose, not on a dead *Choose…* (F-531). */
  function addRule() {
    settled = false;
    const kind = RULE_KINDS.find((option) => options(option.kind, "").length > 0)?.kind ?? "epic";
    pending = { at: set?.rules.length ?? 0, kind };
  }

  /** Why a waiting rule has nothing to choose. A rule of this kind above it already holds one, so the line says *other* and doesn't contradict it (F-531). */
  function noneOf(kind: LinkSetRuleKind): string {
    const other = set?.rules.some((rule) => rule.kind === kind) ? "other " : "";
    if (kind === "epic") return `No ${other}synced epic has tickets under it yet. Try another kind.`;
    if (kind === "parent") return `No ${other}synced ticket has sub-tasks yet. Try another kind.`;
    return kind === "project" ? `No ${other}project has synced yet.` : `No ${other}ticket has synced yet.`;
  }

  /** Whether the waiting rule ended with a value, which keeps focus where it is. */
  let settled = false;
  let rulesEl = $state<HTMLElement | null>(null);

  /** Where focus goes when a rule stops waiting: its value if it took one, its kind if the row stays, else *Add a rule*. */
  function ruleBack(at: number): HTMLElement | null {
    const row = rulesEl?.querySelector<HTMLElement>(`[data-rule="${at}"]`);
    return row?.querySelector<HTMLElement>(settled ? "select[aria-label='Applies to which']" : "select[aria-label='Rule']") ?? rulesEl?.querySelector<HTMLElement>("[data-add-rule]") ?? null;
  }

  /**
   * A rule waiting for its value is an inline editor: focus goes to the value, and `Esc` puts it away first, wherever focus is.
   * In one of its dropdowns one press does it: a blur first, as a text field has, would ask twice.
   */
  const waitingRule = (at: number) =>
    inlineEditor(() => cancelRule(), { focus: "select[aria-label='Applies to which']", back: () => ruleBack(at) });

  function cancelRule() {
    settled = false;
    pending = null;
  }

  function ruleKey(event: KeyboardEvent, waiting: boolean) {
    if (event.key !== "Escape") return;
    // A rule holding its value has no edit to undo, so `Esc` goes on and leaves the page, as the title's does (F-539), not to the body.
    if (!waiting) {
      if (event.target instanceof HTMLSelectElement) event.target.blur();
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    cancelRule();
  }

  /**
   * Take one thing out of the set, at once, with *Undo* on the toast. A second
   * removal while that toast shows replaces it: one toast, and its *Undo* puts
   * back all of them. Each set has its own tally: *Undo* goes to the set it was
   * removed from, whichever set this editor has moved on to (`undoOn`).
   */
  const removedTally = (from: string, name: string) =>
    tally<{ gone: Removed; named: string }>((held) => ({
      text: `Removed ${held.length === 1 ? held[0]!.named : held.length} from ${name}`,
      action: {
        label: "Undo",
        run: () =>
          undoOn(
            from,
            (draft) =>
              putBack(
                draft,
                held.map((one) => one.gone),
              ),
            ports,
          ),
      },
    }));
  let removedFrom: { id: string; toast: ReturnType<typeof removedTally> } | null = null;

  function removedToast(one: { gone: Removed; named: string }) {
    if (!set) return;
    if (removedFrom?.id !== set.id) removedFrom = { id: set.id, toast: removedTally(set.id, set.name) };
    removedFrom.toast(one);
  }

  /**
   * Focus after a *Remove* took its row: the next row's *Remove*, else the row before's, else the panel's way of adding one.
   * So a run of removals is a run of presses, and `Tab` carries on from where the reader was.
   */
  function refocusAfter(pressed: HTMLElement | null): () => void {
    const holder = pressed?.closest<HTMLElement>(".tf-members, .tf-panel") ?? null;
    if (pressed === null || holder === null) return () => {};
    const at = [...holder.querySelectorAll<HTMLElement>(".tf-x")].indexOf(pressed);
    const panel = holder.closest<HTMLElement>(".tf-panel") ?? holder;
    return () => {
      void tick().then(() => {
        const active = document.activeElement;
        if (active !== null && active !== document.body && active !== pressed && active.isConnected) return;
        const rows = [...holder.querySelectorAll<HTMLElement>(".tf-x")];
        (rows[at] ?? rows[at - 1] ?? panel.querySelector<HTMLElement>(".tf-set-add .tf-btn:last-child, [data-add-rule]"))?.focus({ preventScroll: true });
      });
    };
  }

  function removeOne(gone: Removed, named: string, change: (draft: LinkSetDraft) => void, pressed: HTMLElement | null) {
    const refocus = refocusAfter(pressed);
    void save(change, undefined, () => {
      removedToast({ gone, named });
      refocus();
    });
  }

  function dropMember(at: number, named: string, pressed: HTMLElement | null) {
    const item = set?.members[at];
    if (!item) return;
    const { kind, entity_id } = item;
    removeOne({ list: "members", at, item: { kind, entity_id } }, named, (draft) => {
      draft.members = draft.members.filter((held) => held.entity_id !== entity_id);
    }, pressed);
  }

  function dropPattern(at: number, pressed: HTMLElement | null) {
    const item = set?.patterns[at];
    if (!item) return;
    const { kind, query } = item;
    removeOne({ list: "patterns", at, item: { kind, query } }, "a pattern", (draft) => {
      draft.patterns = draft.patterns.filter((held) => !(held.kind === kind && held.query === query));
    }, pressed);
  }

  function dropRule(at: number, pressed: HTMLElement | null) {
    if (pending?.at === at) cancelRule();
    const item = set?.rules[at];
    if (!item) return;
    const { kind, value } = item;
    removeOne({ list: "rules", at, item: { kind, value } }, "a rule", (draft) => {
      draft.rules = draft.rules.filter((held) => !(held.kind === kind && held.value === value));
    }, pressed);
  }

  /** The rows *Applies to* draws: the stored rules, one re-kinded or added waiting for its value. */
  const ruleRows = $derived.by((): (LinkSetRuleRow & { at: number; waiting: boolean })[] => {
    const rows = (set?.rules ?? []).map((rule, at) =>
      pending?.at === at ? { kind: pending.kind, value: "", title: null, at, waiting: true } : { ...rule, at, waiting: false },
    );
    if (pending && pending.at >= rows.length) rows.push({ kind: pending.kind, value: "", title: null, at: rows.length, waiting: true });
    return rows;
  });

  /** The tickets it shows on, one chip a ticket: two sources holding one key are one ticket, as the ticket's page says. */
  const shownOn = $derived(onePerKey(set?.shows_on ?? []));
  const shownKeys = $derived(shownOn.slice(0, SHOWN_KEYS));
  const have = $derived(new Set(set?.members.map((member) => member.entity_id) ?? []));
</script>

<TfCrumbs parts={[{ name: "Link sets", href: "#/sets" }, { name: id === NEW_SET ? NEW_SET_NAME : (set?.name ?? "Link set") }]} {back} />
{#if missing}
  <TfNothingHere message="This link set does not exist. It may have been deleted." href="#/sets" action="All link sets" />
{:else if failed !== null}
  <div class="tf-empty">
    <p class="tf-ifail">Could not read the link set: {failed}</p>
    <button type="button" class="tf-btn" onclick={() => attempt++}>Try again</button>
  </div>
{:else if set === null}
  <div class="tf-cbody"><TfWaiting phase={waitPhase({ drawn: false, reading: true, failed: false })} shape="lines" count={8}><span></span></TfWaiting></div>
{:else}
  <header class="tf-fhead">
    <div>
      <!-- svelte-ignore a11y_missing_content -- `holds` writes its words. -->
      <h1>
        <span
          class="tf-set-title"
          contenteditable="plaintext-only"
          spellcheck="false"
          role="textbox"
          tabindex="0"
          aria-label="Name"
          title="Click to rename"
          bind:this={title}
          {@attach holds(() => set?.name ?? "")}
          onblur={(event) => rename(event, "name")}
          onkeydown={(event) => fieldKey(event, "name")}></span
        >
      </h1>
      <div
        class="tf-sub tf-set-desc"
        contenteditable="plaintext-only"
        role="textbox"
        tabindex="0"
        aria-label="Description"
        data-placeholder="Describe what this set is for."
        {@attach holds(() => set?.description ?? "")}
        onblur={(event) => rename(event, "description")}
        onkeydown={(event) => fieldKey(event, "description")}
      ></div>
    </div>
    <!-- Nothing is saved yet, so there is nothing to delete. -->
    {#if set.id !== ""}<div class="tf-acts"><button type="button" class="tf-btn tf-danger" onclick={() => (deleting = true)}>Delete…</button></div>{/if}
  </header>
  <div class="tf-cbody tf-set">
    <div class="tf-set-panels">
      {#each MEMBER_KINDS as kind (kind)}
        {@const members = set.members.filter((member) => member.kind === kind)}
        {@const patterns = set.patterns.map((pattern, at) => ({ ...pattern, at })).filter((pattern) => pattern.kind === kind)}
        <article class="tf-panel" aria-label={KIND_LABEL[kind]}>
          <header>
            <h3>{KIND_LABEL[kind]}</h3>
            <span class="tf-count">{itemsOf({ members, patterns }).length}</span>
            <span class="tf-set-add tf-trail"
              >{#if PATTERN_KINDS.includes(kind)}<button
                  type="button"
                  class="tf-btn tf-sm tf-ghost"
                  onclick={() => (patterning = kind)}>Add a pattern…</button
                >{/if}<button type="button" class="tf-btn tf-sm" onclick={() => (picking = kind)}>Add…</button></span
            >
          </header>
          <div class="tf-members">
            {#each patterns as pattern (pattern.at)}
              <div class="tf-member tf-pattern">
                <details class="tf-set-fold">
                  <summary>Pattern: <code>{patternShown(kind, pattern.query)}</code> <span class="tf-dim">({counted(pattern.count, "match", "matches")} now)</span></summary>
                  <ul>
                    {#each pattern.matches as row (row.entity_id)}<li><a href={entityAddress(kind, row.entity_id)}>{row.title ?? keyOf(row.entity_id)}</a></li>{/each}
                    {#if pattern.count > pattern.matches.length}<li class="tf-dim">and {pattern.count - pattern.matches.length} more</li>{/if}
                  </ul>
                </details>
                <button
                  type="button"
                  class="tf-btn tf-sm tf-ghost tf-x"
                  aria-label="Remove the pattern {patternShown(kind, pattern.query)}"
                  onclick={(event) => dropPattern(pattern.at, event.currentTarget)}>Remove</button
                >
              </div>
            {/each}
            {#each members as member (member.entity_id)}
              {@const state = memberState(member)}
              <div class="tf-member">
                <!-- A dot only where the source reports a state: tone is for state, and a row of grey dots says nothing. -->
                {#if state}<span class="tf-dot {toneClass(state)}" title={stateWord(state.text)}></span>{/if}
                <a class="tf-member-name" class:tf-mono={kind === "repo"} href={entityAddress(kind, member.entity_id)}>{member.title ?? keyOf(member.entity_id)}</a>
                {#if member.title === null}<span class="tf-dim">not synced</span>{:else if member.line}<span class="tf-dim tf-member-line">{memberLine(member)}</span>{/if}
                <button
                  type="button"
                  class="tf-btn tf-sm tf-ghost tf-x"
                  aria-label="Remove {member.title ?? member.entity_id}"
                  onclick={(event) => dropMember(set?.members.findIndex((held) => held.entity_id === member.entity_id) ?? -1, member.title ?? keyOf(member.entity_id), event.currentTarget)}
                  >Remove</button
                >
              </div>
            {/each}
            {#if members.length === 0 && patterns.length === 0}<div class="tf-member tf-dim">No {KIND_LABEL[kind].toLowerCase()} yet.</div>{/if}
          </div>
        </article>
      {/each}
    </div>
    <aside class="tf-side">
      <div class="tf-panel" bind:this={rulesEl}>
        <header><h3>Applies to</h3></header>
        {#each ruleRows as rule (rule.at)}
          {@const count = rule.waiting ? undefined : counts[countKey(rule)]}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="tf-rule" data-rule={rule.at} onkeydown={(event) => ruleKey(event, rule.waiting)} {@attach rule.waiting ? waitingRule(rule.at) : null}>
            <select aria-label="Rule" value={rule.kind} onchange={(event) => rekind(rule.at, event.currentTarget.value as LinkSetRuleKind)}>
              {#each RULE_KINDS as option (option.kind)}<option value={option.kind}>{option.label}</option>{/each}
            </select>
            <select aria-label="Applies to which" class:tf-mono={rule.kind === "project" && rule.value !== ""} value={rule.value} onchange={(event) => choose(rule.at, rule.kind, event.currentTarget.value)}>
              {#if rule.waiting}<option value="">Choose…</option>{/if}
              {#each options(rule.kind, rule.value) as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
            </select>
            <div class="tf-rule-foot">
              {#if rule.waiting}<span class="tf-dim tf-rule-n">{options(rule.kind, rule.value).length === 0 ? noneOf(rule.kind) : "Choose what it applies to."}</span>
              {:else if count !== undefined && count >= 0}<span class="tf-dim tf-rule-n">{counted(count, "ticket", "tickets")}</span>{/if}
              <button type="button" class="tf-btn tf-sm tf-ghost tf-x" onclick={(event) => dropRule(rule.at, event.currentTarget)}>{rule.at >= set.rules.length ? "Cancel" : "Remove"}</button>
            </div>
          </div>
        {:else}
          <div class="tf-rule tf-rule-none tf-dim">No rule yet. The set shows nowhere until it has one.</div>
        {/each}
        <div class="tf-rule-add">
          <button
            type="button"
            class="tf-btn tf-sm"
            data-add-rule
            disabled={pending !== null}
            title={pending !== null ? "Choose what the rule above applies to first" : undefined}
            onclick={addRule}>Add a rule</button
          >
        </div>
      </div>
      <div class="tf-panel">
        <header><h3>Shows on {counted(shownOn.length, "ticket", "tickets")} now</h3></header>
        {#if shownOn.length > 0}
          <div class="tf-roll">{shownOn.filter((ticket) => ticket.mine).length} of them yours, outlined. New tickets join as soon as they match.</div>
        {:else}
          <div class="tf-roll">It shows on tickets once it has a rule that matches some.</div>
        {/if}
        <div class="tf-keys">
          {#each shownKeys as ticket (keyOf(ticket.entity_id))}
            <a href={entityAddress("ticket", ticket.entity_id)} class:tf-mine={ticket.mine} title={ticket.title}>{keyOf(ticket.entity_id)}</a>
          {/each}
          {#if shownOn.length > SHOWN_KEYS}<span class="tf-dim">and {shownOn.length - SHOWN_KEYS} more</span>{/if}
        </div>
      </div>
      <p class="tf-dim tf-set-note">A ticket can hide one item for itself from its connections drawer. Editing the set changes every job at once.</p>
    </aside>
  </div>
  {#if picking !== null}
    {@const kind = picking}
    <TfSetPicker
      {kind}
      setName={set.name}
      {have}
      read={() => ports.candidates(kind)}
      onadd={(picked) => added(kind, picked)}
      onclose={() => (picking = null)}
    />
  {/if}
  {#if patterning !== null}
    {@const kind = patterning}
    <TfSetPattern
      {kind}
      setName={set.name}
      preview={(query) => ports.previewLinkSetPattern({ kind, query })}
      onadd={(query) => addedPattern(kind, query)}
      onclose={() => (patterning = null)}
    />
  {/if}
  {#if deleting}
    {@const doomed = set}
    <TfDialog title="Delete {doomed.name}?" onclose={() => (deleting = false)}>
      {#snippet body()}
        <p>
          It stops showing on {counted(shownOn.length, "ticket", "tickets")} now, and on every ticket its rules would
          match later. Links drawn on tickets by hand stay.
        </p>
      {/snippet}
      {#snippet footer()}
        <button type="button" class="tf-btn tf-ghost" onclick={() => (deleting = false)}>Cancel</button>
        <button type="button" class="tf-btn tf-primary tf-danger" onclick={() => void remove()}>Delete link set</button>
      {/snippet}
    </TfDialog>
  {/if}
{/if}
