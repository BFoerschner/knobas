<!--
  What the backend made of the box (`.fbar`/`.fchip`, round 3, lines 402-409).

  **`response.interpreted` is the only source of these chips.** The frontend
  never re-parses the box: ruling P2 puts the whole grammar in
  `crates/knobas-search/src/query.rs`, and a second implementation here would
  disagree with the one that produced the rows — silently, and in the direction
  that looks right.

  `unknown_tokens` are the point of the bar rather than an afterthought. A
  filter that did nothing must never be invisible: `env:prod` greyed out with
  *the estate arrives in M4* is a launcher telling the truth; `env:prod`
  silently ignored is a launcher showing the wrong result set.
-->
<script lang="ts">
  import type { ParsedQuery } from "../ipc";
  import type { CredentialHealth } from "../ipc/sources";
  import { isActionable } from "../shell/health.svelte";

  let { interpreted, sources }: { interpreted: ParsedQuery; sources: CredentialHealth[] } =
    $props();

  /** The prefix chip, in the words the `?` card uses. */
  const PREFIX_LABEL: Record<string, string> = {
    action: "actions",
    ticket: "tickets",
    person: "people",
    source: "source",
    time: "time",
    note: "notes",
    list: "smart list",
    asset: "assets",
    help: "help",
  };

  /**
   * Why a token did nothing, in a sentence rather than a shrug.
   *
   * Derived from the token's own shape, which is the only thing the response
   * carries — `unknown_tokens` is a list of strings by design (interfaces
   * §2.4), so the reason is the launcher's to phrase. Where no specific reason
   * fits, the generic one still says *this filtered nothing*, which is the
   * fact that matters.
   */
  function reason(token: string): string {
    const key = token.split(":")[0]?.toLowerCase() ?? "";
    if (token.startsWith("/")) return `no source matches ${token}`;
    if (key === "env" || key === "health") return "the estate arrives with assets";
    if (key === "type" || key === "kind") return "no such kind, or no value yet";
    if (key === "updated") return "not a duration — try today, 7d, 2w";
    return "this filtered nothing";
  }

  /** A configured source's health, for the dot on its chip. */
  function health(id: string): CredentialHealth | undefined {
    return sources.find((source) => source.source_id === id);
  }

  const chips = $derived.by(() => {
    const out: { key: string; text: string; title: string; fail: boolean }[] = [];
    if (interpreted.prefix) {
      out.push({
        key: `prefix:${interpreted.prefix}`,
        text: PREFIX_LABEL[interpreted.prefix] ?? interpreted.prefix,
        title: "the prefix the box opened with",
        fail: false,
      });
    }
    for (const id of interpreted.filters.sources) {
      const state = health(id)?.state;
      out.push({
        key: `source:${id}`,
        text: id,
        title: state ? `source · credential ${state}` : "source",
        // The same rule the row's provenance line and the board's source
        // strip branch on, spelled in `shell/health.svelte` and nowhere else:
        // a chip's dot and a row's complaint disagreeing about one source is
        // indistinguishable from a bug. `state` is `undefined` when the store
        // has no row for this id yet, which is a case the rule answers itself
        // (#72) — a guard here would be a second place the rule is written.
        fail: isActionable(state),
      });
    }
    for (const kind of interpreted.filters.kinds) {
      out.push({ key: `kind:${kind}`, text: kind, title: "kind", fail: false });
    }
    if (interpreted.filters.updated_within_days !== null) {
      out.push({
        key: "updated",
        text: `updated ≤ ${interpreted.filters.updated_within_days} days`,
        title: "changed within",
        fail: false,
      });
    }
    if (interpreted.filters.mine) {
      out.push({ key: "mine", text: "@me", title: "your configured accounts", fail: false });
    }
    // Named people, and only named people. `mine` is the chip above; the
    // usernames it stands for never cross the bridge, so there is nothing here
    // the user did not type.
    for (const author of interpreted.filters.authors) {
      out.push({ key: `author:${author}`, text: author, title: "author", fail: false });
    }
    return out;
  });
</script>

{#if chips.length > 0 || interpreted.unknown_tokens.length > 0}
  <div class="fbar">
    {#each chips as chip (chip.key)}
      <span class="fchip on" class:fail={chip.fail} title={chip.title}>{chip.text}</span>
    {/each}
    {#each interpreted.unknown_tokens as token (token)}
      <span class="fchip off" title={reason(token)}>{token} — {reason(token)}</span>
    {/each}
  </div>
{/if}

<style>
  .fbar {
    display: flex;
    gap: 4px;
    row-gap: 5px;
    align-items: center;
    padding: 7px 10px;
    border-bottom: 1px solid var(--hair);
    flex-wrap: wrap;
    flex: none;
  }
  .fchip {
    flex: none;
    white-space: nowrap;
    font: 400 11px var(--mono);
    border: 1px solid var(--hair);
    border-radius: 2px;
    padding: 2px 5px;
  }
  .fchip.on {
    color: var(--text);
    border-color: var(--muted);
    background: var(--raised);
  }
  .fchip.on.fail {
    color: var(--fail);
    border-color: var(--fail);
  }
  /* Greyed *and* dashed: a filter that did nothing has to be legible as
     "present but inert" at a glance, not just dimmer than its neighbours. */
  .fchip.off {
    color: var(--faint);
    border-style: dashed;
    border-color: var(--hair2);
  }
</style>
