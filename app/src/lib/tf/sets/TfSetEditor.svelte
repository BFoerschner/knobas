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
    rule*.
  - *Shows on N tickets now*: how many are yours, and the keys as links to
    their jobs (yours in yellow), the first 40 then "and N more".

  Every change saves the whole set (`save_link_set`) and draws what the save
  answered, as prototype-6 has no Save button: a set is live, and every job it
  applies to reads the change on its next read.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type {
    LinkSetDetail,
    LinkSetDraft,
    LinkSetMemberKind,
    LinkSetRule,
    LinkSetRuleKind,
    LinkSetRuleRow,
  } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { keyOf } from "../job/job";
  import { entityAddress } from "../address";
  import { draftOf } from "./apply";
  import type { Candidate, RuleChoices, SetPorts } from "./ports";
  import { counted } from "../counted";
  import { dotClass, itemsOf, KIND_LABEL, MEMBER_KINDS, PATTERN_KINDS, RULE_KINDS, SHOWN_KEYS } from "./sets";
  import TfSetPattern from "./TfSetPattern.svelte";
  import TfSetPicker from "./TfSetPicker.svelte";

  let {
    ready,
    id,
    ports,
  }: {
    ready: boolean;
    /** The set's id, from `#/sets/<id>`. */
    id: string;
    ports: SetPorts;
  } = $props();

  let set = $state<LinkSetDetail | null>(null);
  let failed = $state<string | null>(null);
  let missing = $state(false);
  let choices = $state<RuleChoices | null>(null);
  /** Live ticket counts per rule, by `kind|value`. */
  let counts = $state<Record<string, number>>({});
  /** A rule being added or re-kinded, until its value is chosen. */
  let pending = $state<{ at: number; kind: LinkSetRuleKind } | null>(null);
  let picking = $state<LinkSetMemberKind | null>(null);
  /** The kind whose *Add a pattern…* dialog is open. */
  let patterning = $state<LinkSetMemberKind | null>(null);
  /** The saves asked for so far, in order (see `save`). */
  let saves: Promise<void> = Promise.resolve();

  $effect(() => {
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
  function save(change: (draft: LinkSetDraft) => void, toast?: string): Promise<void> {
    const target = set?.id;
    saves = saves.then(async () => {
      if (!set || set.id !== target) return;
      const draft = draftOf(set);
      change(draft);
      try {
        set = await ports.saveLinkSet(draft);
        if (toast) push({ text: toast });
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

  function enterLeaves(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      (event.currentTarget as HTMLElement).blur();
    }
  }

  async function remove() {
    if (!set) return;
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

  function dropRule(at: number) {
    if (pending?.at === at) pending = null;
    if (at >= (set?.rules.length ?? 0)) return;
    void save((draft) => draft.rules.splice(at, 1));
  }

  /** The rows *Applies to* draws: the stored rules, one re-kinded or added waiting for its value. */
  const ruleRows = $derived.by((): (LinkSetRuleRow & { at: number; waiting: boolean })[] => {
    const rows = (set?.rules ?? []).map((rule, at) =>
      pending?.at === at ? { kind: pending.kind, value: "", title: null, at, waiting: true } : { ...rule, at, waiting: false },
    );
    if (pending && pending.at >= rows.length) rows.push({ kind: pending.kind, value: "", title: null, at: rows.length, waiting: true });
    return rows;
  });

  const shownKeys = $derived(set?.shows_on.slice(0, SHOWN_KEYS) ?? []);
  const have = $derived(new Set(set?.members.map((member) => member.entity_id) ?? []));
</script>

{#if missing}
  <div class="tf-empty">No link set is called {id}. <a href="#/sets">All link sets</a></div>
{:else if failed !== null}
  <p class="tf-ifail">Could not read the link set: {failed}</p>
{:else if set !== null}
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
    <div class="tf-acts"><button type="button" class="tf-btn tf-danger" onclick={() => void remove()}>Delete</button></div>
  </header>
  <div class="tf-cbody tf-set">
    <div class="tf-set-panels">
      {#each MEMBER_KINDS as kind (kind)}
        {@const members = set.members.filter((member) => member.kind === kind)}
        {@const patterns = set.patterns.map((pattern, at) => ({ ...pattern, at })).filter((pattern) => pattern.kind === kind)}
        <article class="tf-panel" aria-label={KIND_LABEL[kind]}>
          <header>
            <h3>{KIND_LABEL[kind]}</h3>
            <span class="tf-dim">{itemsOf({ members, patterns }).length}</span>
            <span class="tf-set-add"
              ><button type="button" class="tf-btn tf-sm" onclick={() => (picking = kind)}>Add…</button>{#if PATTERN_KINDS.includes(kind)}<button
                  type="button"
                  class="tf-btn tf-sm tf-ghost"
                  onclick={() => (patterning = kind)}>Add a pattern…</button
                >{/if}</span
            >
          </header>
          <div class="tf-members">
            {#each patterns as pattern (pattern.at)}
              <div class="tf-member tf-pattern">
                <details>
                  <summary>Pattern: <code>{pattern.query}</code> <span class="tf-dim">({pattern.count} match now)</span></summary>
                  <ul>
                    {#each pattern.matches as row (row.entity_id)}<li>{row.title ?? keyOf(row.entity_id)}</li>{/each}
                    {#if pattern.count > pattern.matches.length}<li class="tf-dim">and {pattern.count - pattern.matches.length} more</li>{/if}
                  </ul>
                </details>
                <button
                  type="button"
                  class="tf-btn tf-sm tf-ghost tf-x"
                  aria-label="Remove the pattern {pattern.query}"
                  onclick={() => void save((draft) => draft.patterns.splice(pattern.at, 1))}>Remove</button
                >
              </div>
            {/each}
            {#each members as member (member.entity_id)}
              <div class="tf-member">
                <span class="tf-dot {dotClass(member.state)}"></span>
                <span class:tf-mono={kind === "repo" || kind === "asset"}>{member.title ?? keyOf(member.entity_id)}</span>
                {#if member.title === null}<span class="tf-dim">not synced</span>{:else if member.line}<span class="tf-dim tf-member-line">{member.line}</span>{/if}
                <button
                  type="button"
                  class="tf-btn tf-sm tf-ghost tf-x"
                  aria-label="Remove {member.title ?? member.entity_id}"
                  onclick={() => void save((draft) => (draft.members = draft.members.filter((held) => held.entity_id !== member.entity_id)))}
                  >Remove</button
                >
              </div>
            {/each}
            {#if members.length === 0 && patterns.length === 0}<div class="tf-member tf-dim">Nothing yet.</div>{/if}
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
            {#if count !== undefined && count >= 0}<span class="tf-dim tf-rule-n">{counted(count, "ticket", "tickets")}</span>{/if}
            <button type="button" class="tf-btn tf-sm tf-ghost tf-x" onclick={() => dropRule(rule.at)}>Remove</button>
          </div>
        {:else}
          <div class="tf-rule tf-dim">No rule yet. The set shows nowhere until it has one.</div>
        {/each}
        <div class="tf-rule-add">
          <button type="button" class="tf-btn tf-sm" disabled={pending !== null} onclick={addRule}>Add a rule</button>
        </div>
      </div>
      <div class="tf-panel">
        <header><h3>Shows on {counted(set.shows_on.length, "ticket", "tickets")} now</h3></header>
        <div class="tf-roll">{set.shows_on.filter((ticket) => ticket.mine).length} of them yours. New tickets join as soon as they match.</div>
        <div class="tf-keys">
          {#each shownKeys as ticket (ticket.entity_id)}
            <a href={entityAddress("ticket", ticket.entity_id)} class:tf-mine={ticket.mine} title={ticket.title}>{keyOf(ticket.entity_id)}</a>
          {/each}
          {#if set.shows_on.length > SHOWN_KEYS}<span class="tf-dim">and {set.shows_on.length - SHOWN_KEYS} more</span>{/if}
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
{/if}
