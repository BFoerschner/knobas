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
    rule*. A rule waiting for its value says so and closes first on `Esc`.
  - *Shows on N tickets now*: how many are yours, and the keys as links to
    their jobs (yours in yellow), the first 40 then "and N more".

  Every change saves the whole set (`save_link_set`) and draws what the save
  answered, as prototype-6 has no Save button: a set is live, and every job it
  applies to reads the change on its next read. A *Remove* acts at once, so it
  toasts with *Undo*, and removals in a row collapse into one toast (the
  removal rule).
-->
<script lang="ts">
  import { untrack } from "svelte";

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
  import { push, tally } from "../../shell/toasts.svelte";
  import { keyOf } from "../job/job";
  import { layers } from "../layers.svelte";
  import { entityAddress } from "../address";
  import type { EscTarget } from "../escape";
  import TfDialog from "../settings/TfDialog.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfCrumbs from "../TfCrumbs.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";
  import { draftOf } from "./apply";
  import type { Candidate, RuleChoices, SetPorts } from "./ports";
  import { counted } from "../counted";
  import { assetLine, itemsOf, KIND_LABEL, MEMBER_KINDS, onePerKey, PATTERN_KINDS, putBack, RULE_KINDS, SHOWN_KEYS, type Removed } from "./sets";
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
      if (!set || set.id !== target) return;
      const draft = draftOf(set);
      change(draft);
      try {
        set = await ports.saveLinkSet(draft);
        if (toast) push({ text: toast });
        saved?.();
      } catch (error) {
        push({ text: ipcErrorMessage(error), tone: "err" });
      }
    });
    return saves;
  }

  function rename(event: FocusEvent, field: "name" | "description") {
    const text = (event.currentTarget as HTMLElement).textContent.trim();
    if (!set || text === set[field]) return;
    if (field === "name" && text === "") {
      (event.currentTarget as HTMLElement).textContent = set.name;
      return;
    }
    void save((draft) => (draft[field] = text));
  }

  /** A set just made has the name the button gave it: its name is selected on arrival, to type over. */
  function selectIfNew(el: HTMLElement) {
    if (el.textContent?.trim() !== "New link set") return;
    el.focus();
    const range = document.createRange();
    range.selectNodeContents(el);
    const selection = getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  }

  function enterLeaves(event: KeyboardEvent) {
    if (event.key === "Escape" && set) {
      // Esc in a field that edits in place puts the old words back, and the field keeps the press.
      const el = event.currentTarget as HTMLElement;
      el.textContent = el.getAttribute("aria-label") === "Name" ? set.name : set.description;
      event.stopPropagation();
      el.blur();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      (event.currentTarget as HTMLElement).blur();
    }
  }

  async function remove() {
    if (!set) return;
    deleting = false;
    const gone = set.id;
    await saves;
    try {
      await ports.deleteLinkSet(gone);
      push({ text: "Deleted the link set. Links drawn on tickets by hand stay." });
      location.hash = "#/sets";
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    }
  }

  function addedPattern(kind: LinkSetMemberKind, query: string) {
    patterning = null;
    void save((draft) => draft.patterns.push({ kind, query }), `Added a pattern to ${set?.name ?? ""}`);
  }

  function added(kind: LinkSetMemberKind, picked: Candidate[]) {
    picking = null;
    const name = set?.name ?? "";
    void save(
      (draft) => draft.members.push(...picked.map((row) => ({ kind, entity_id: row.entity_id }))),
      `Added ${picked.length} to ${name}`,
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
    const rule = set?.rules[at];
    if (!rule || rule.kind === kind) return;
    pending = { at, kind };
  }

  function choose(at: number, kind: LinkSetRuleKind, value: string) {
    if (!value) return;
    const replacing = pending?.at === at && at < (set?.rules.length ?? 0);
    pending = null;
    void save((draft) => {
      const rule = { kind, value };
      if (draft.rules.some((held) => held.kind === kind && held.value === value)) {
        if (replacing) draft.rules.splice(at, 1);
        return;
      }
      if (at < draft.rules.length) draft.rules.splice(at, 1, rule);
      else draft.rules.push(rule);
    });
  }

  function addRule() {
    pending = { at: set?.rules.length ?? 0, kind: "epic" };
  }

  /** A rule waiting for its value is an inline editor: `Esc` puts it away first, wherever focus is. */
  $effect(() => {
    if (pending === null) return;
    return untrack(() => layers.add({ kind: "dialog", close: () => (pending = null) }));
  });

  /**
   * Take one thing out of the set, at once, with *Undo* on the toast. A second
   * removal while that toast shows replaces it: one toast, and its *Undo* puts
   * back all of them.
   */
  const removedToast = tally<{ gone: Removed; named: string }>((held) => ({
    text: `Removed ${held.length === 1 ? held[0]!.named : held.length} from ${set?.name ?? "the set"}`,
    action: {
      label: "Undo",
      run: () =>
        void save((draft) =>
          putBack(
            draft,
            held.map((one) => one.gone),
          ),
        ),
    },
  }));

  function removeOne(gone: Removed, named: string, change: (draft: LinkSetDraft) => void) {
    void save(change, undefined, () => removedToast({ gone, named }));
  }

  function dropMember(at: number, named: string) {
    const item = set?.members[at];
    if (!item) return;
    const { kind, entity_id } = item;
    removeOne({ list: "members", at, item: { kind, entity_id } }, named, (draft) => {
      draft.members = draft.members.filter((held) => held.entity_id !== entity_id);
    });
  }

  function dropPattern(at: number) {
    const item = set?.patterns[at];
    if (!item) return;
    const { kind, query } = item;
    removeOne({ list: "patterns", at, item: { kind, query } }, "a pattern", (draft) => {
      draft.patterns = draft.patterns.filter((held) => !(held.kind === kind && held.query === query));
    });
  }

  function dropRule(at: number) {
    if (pending?.at === at) pending = null;
    const item = set?.rules[at];
    if (!item) return;
    const { kind, value } = item;
    removeOne({ list: "rules", at, item: { kind, value } }, "a rule", (draft) => {
      draft.rules = draft.rules.filter((held) => !(held.kind === kind && held.value === value));
    });
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

<TfCrumbs parts={[{ name: "Link sets", href: "#/sets" }, { name: set?.name ?? "Link set" }]} {back} />
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
    {#key `${set.id}|${set.name}|${set.description}`}
    <div>
      <h1>
        <span
          class="tf-set-title"
          contenteditable="true"
          spellcheck="false"
          role="textbox"
          tabindex="0"
          aria-label="Name"
          title="Click to rename"
          {@attach selectIfNew}
          onblur={(event) => rename(event, "name")}
          onkeydown={enterLeaves}>{set.name}</span
        >
      </h1>
      <div
        class="tf-sub tf-set-desc"
        contenteditable="true"
        role="textbox"
        tabindex="0"
        aria-label="Description"
        data-placeholder="Describe what this set is for."
        onblur={(event) => rename(event, "description")}
        onkeydown={enterLeaves}
      >{set.description}</div>
    </div>
    {/key}
    <div class="tf-acts"><button type="button" class="tf-btn tf-danger" onclick={() => (deleting = true)}>Delete…</button></div>
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
                  <summary>Pattern: <code>{pattern.query}</code> <span class="tf-dim">({counted(pattern.count, "match", "matches")} now)</span></summary>
                  <ul>
                    {#each pattern.matches as row (row.entity_id)}<li><a href={entityAddress(kind, row.entity_id)}>{row.title ?? keyOf(row.entity_id)}</a></li>{/each}
                    {#if pattern.count > pattern.matches.length}<li class="tf-dim">and {pattern.count - pattern.matches.length} more</li>{/if}
                  </ul>
                </details>
                <button
                  type="button"
                  class="tf-btn tf-sm tf-ghost tf-x"
                  aria-label="Remove the pattern {pattern.query}"
                  onclick={() => dropPattern(pattern.at)}>Remove</button
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
                  onclick={() => dropMember(set?.members.findIndex((held) => held.entity_id === member.entity_id) ?? -1, member.title ?? keyOf(member.entity_id))}
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
      <div class="tf-panel">
        <header><h3>Applies to</h3></header>
        {#each ruleRows as rule (rule.at)}
          {@const count = rule.waiting ? undefined : counts[countKey(rule)]}
          <div class="tf-rule">
            <select aria-label="Rule" value={rule.kind} onchange={(event) => rekind(rule.at, event.currentTarget.value as LinkSetRuleKind)}>
              {#each RULE_KINDS as option (option.kind)}<option value={option.kind}>{option.label}</option>{/each}
            </select>
            <select aria-label="Applies to which" value={rule.value} onchange={(event) => choose(rule.at, rule.kind, event.currentTarget.value)}>
              {#if rule.waiting}<option value="">Choose…</option>{/if}
              {#each options(rule.kind, rule.value) as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
            </select>
            <div class="tf-rule-foot">
              {#if rule.waiting}<span class="tf-dim tf-rule-n">Choose what it applies to.</span>
              {:else if count !== undefined && count >= 0}<span class="tf-dim tf-rule-n">{counted(count, "ticket", "tickets")}</span>{/if}
              <button type="button" class="tf-btn tf-sm tf-ghost tf-x" onclick={() => dropRule(rule.at)}>{rule.at >= set.rules.length ? "Cancel" : "Remove"}</button>
            </div>
          </div>
        {:else}
          <div class="tf-rule tf-rule-none tf-dim">No rule yet. The set shows nowhere until it has one.</div>
        {/each}
        <div class="tf-rule-add">
          <button
            type="button"
            class="tf-btn tf-sm"
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
