<!--
  Settings › Appearance › Code colours (ADR-0051; `docs/decisions/2026-11-21-a-theme-is-yours-and-code-is-coloured-by-two-engines.md`,
  `docs/decisions/2026-11-21-settings-has-subtrees.md`): the colours of code and diffs. The window's side stays on
  Appearance (*Theme*), an app-wide setting; this page reads it to ring the slot drawn now. Top to bottom: the two slots, *While light* and *While dark*, each a choice of that side's themes with six chips of its
  colours, the slot the window draws now ringed in ink, and then the editor of the theme last chosen or edited
  (`TfThemeEditor.svelte`). *Import…* sits at the header's right edge. Needs no database: everything here is this
  computer's, in `localStorage` (`syntax-theme.svelte.ts`, `theme.svelte.ts`).
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import { renderPort, type RenderPort } from "../../render/port";
  import { ROLE_NAMES, ROLES, type Role } from "../../render/roles";
  import { push, undoable } from "../../shell/toasts.svelte";
  import type { EngineId } from "../../render/engines";
  import { highlighter as appHighlighter, type HighlighterChoice } from "../highlighter.svelte";
  import { syntaxThemes, type SyntaxThemes } from "../syntax-theme.svelte";
  import { theme as windowTheme, type Theme as WindowTheme } from "../theme.svelte";
  import { fixedRole, INK } from "../theme/contrast";
  import { cssVars } from "../theme/paint";
  import { importThemes, type Theme, type ThemeKind } from "../theme/themes";
  import TfThemeEditor from "./TfThemeEditor.svelte";

  let {
    themes = syntaxThemes,
    mode = windowTheme,
    highlighter = appHighlighter,
    render = renderPort(),
    prefersDark = () => typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia("(prefers-color-scheme: dark)").matches,
  }: { themes?: SyntaxThemes; mode?: WindowTheme; highlighter?: HighlighterChoice; render?: RenderPort; prefersDark?: () => boolean } = $props();

  /** The three highlighters, each with its line: speed, then exactness and reach, then age (the re-ruling, section 1). */
  const ENGINES: readonly (readonly [EngineId, string, string])[] = [
    ["pierre", "Pierre Highlights", "Fastest. Colours arrive with the text, in a 50,000-line file too. 152 languages. New, since September 2026."],
    ["tanstack", "TanStack Highlight", "Fast and small. 79 languages. Made for short code in documents, so YAML and shell are less exact. Since June 2026."],
    ["shiki", "Shiki", "Most exact. The grammars VS Code uses, 242 languages. Slow in long files, where colours follow the text by seconds. Since 2018."],
  ];
  const engineName = (id: EngineId) => ENGINES.find(([key]) => key === id)![1];
  /** Who colours while the chosen one cannot be loaded: the first of its order that loaded. */
  const standIn = (id: EngineId) => engineName((["pierre", "tanstack", "shiki"] as EngineId[]).find((e) => e !== id && !highlighter.failed.has(e)) ?? "pierre");

  const SLOTS: readonly (readonly [ThemeKind, string])[] = [
    ["light", "While light"],
    ["dark", "While dark"],
  ];
  /** The six roles a slot's chips show. */
  const CHIPS: readonly Role[] = ["com", "kw", "str", "num", "fn", "ty"];

  // The system's side, followed while the window is *Like the system*.
  let systemDark = $state(untrack(() => prefersDark()));
  onMount(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const heard = () => (systemDark = query.matches);
    query.addEventListener("change", heard);
    return () => query.removeEventListener("change", heard);
  });
  const drawnSide = $derived<ThemeKind>(mode.choice === "system" ? (systemDark ? "dark" : "light") : mode.choice);

  /** The theme in the editor: the one last chosen or edited, at first the one the window draws now. */
  let editingId = $state(untrack(() => themes.slot(drawnSide)));
  const editing = $derived(themes.find(editingId) ?? themes.drawn(drawnSide));

  function choose(id: string): void {
    themes.choose(id);
    editingId = id;
  }

  function duplicate(of: Theme): void {
    const { id: _id, ...rest } = of;
    const copy = themes.add({ ...rest, name: `${of.name} copy`, from: of.name });
    if (copy === null) return void push({ text: "knobas keeps 64 themes of your own. Delete one to make another." });
    // A person who copies a theme means to see their changes: it goes in its kind's slot.
    themes.choose(copy.id);
    editingId = copy.id;
  }

  function remove(gone: Theme): void {
    const undo = themes.remove(gone.id);
    if (undo === null) return;
    if (editingId === gone.id) editingId = themes.slot(gone.kind);
    undoable(`Deleted ${gone.name}.`, () => {
      undo();
      editingId = gone.id;
    });
  }

  // ---------- import ----------
  let picker = $state<HTMLInputElement | null>(null);

  /** The roles of `t` under 4.5:1. */
  const lowRoles = (t: Theme) => ROLES.filter((role) => fixedRole(t.kind, role, t.syntax[role], t.diff ?? {}) !== null);

  function fixAll(ids: readonly string[]): void {
    for (const id of ids)
      themes.update(id, (t) => {
        const syntax = { ...t.syntax };
        for (const role of lowRoles(t)) syntax[role] = fixedRole(t.kind, role, t.syntax[role], t.diff ?? {})!;
        return { ...t, syntax };
      });
    themes.flush();
  }

  async function imported(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    const result = importThemes(await file.text());
    if ("refused" in result) return void push({ text: result.refused });
    const added: Theme[] = [];
    for (const one of result.themes) {
      const t = themes.add(one);
      if (t) added.push(t);
    }
    if (added.length === 0) return void push({ text: "knobas keeps 64 themes of your own. Delete one to import another." });
    editingId = added[0]!.id;
    const low = added.reduce((n, t) => n + lowRoles(t).length, 0);
    const what = added.length === 1 ? `${added[0]!.name} as a ${added[0]!.kind} theme` : `${added.length} themes`;
    const tail = low === 0 ? "" : low === 1 ? " 1 colour is under 4.5:1." : ` ${low} colours are under 4.5:1.`;
    const ids = added.map((t) => t.id);
    if (low === 0) push({ text: `Imported ${what}.` });
    else push({ text: `Imported ${what}.${tail}`, action: { label: "Fix all", run: () => fixAll(ids) } });
  }
</script>

<header class="tf-stg-h tf-thm-top">
  <div>
    <h1>Code colours</h1>
    <p>The colours of code and diffs. One theme draws while the window is light, one while it is dark. Kept on this computer.</p>
  </div>
  <div class="tf-acts">
    <button type="button" class="tf-btn" onclick={() => picker?.click()}>Import…</button>
    <input type="file" accept=".json,application/json" hidden bind:this={picker} onchange={imported} aria-label="Theme file" />
  </div>
</header>
<div class="tf-stg-row tf-thm-eng">
  <div class="tf-stg-l">Highlighter</div>
  <div class="tf-stg-c">
    <div class="tf-radio" role="radiogroup" aria-label="Highlighter">
      {#each ENGINES as [key, name, line] (key)}
        <label>
          <input type="radio" name="tf-thm-engine" value={key} checked={highlighter.chosen === key} onchange={() => highlighter.choose(key)} />
          <span>{name}<small>{line}</small></span>
        </label>
      {/each}
    </div>
    {#if highlighter.failed.has(highlighter.chosen)}
      <div class="tf-thm-engfail" role="status">
        {engineName(highlighter.chosen)} could not be loaded. {standIn(highlighter.chosen)} is colouring instead.
        <button type="button" class="tf-btn tf-sm" onclick={() => highlighter.retry(highlighter.chosen)}>Try again</button>
      </div>
    {/if}
    <p class="tf-stg-note">A language the chosen highlighter does not know is coloured by one of the other two. Under Pierre Highlights, SQL is coloured by TanStack Highlight.</p>
  </div>
</div>
<div class="tf-thm-slots">
  {#each SLOTS as [kind, label] (kind)}
    {@const held = themes.drawn(kind)}
    <div class="tf-thm-slot" class:tf-cur={kind === drawnSide} data-slot={kind}>
      <h3>{label}</h3>
      <select
        class="tf-thm-sel"
        aria-label="Theme {label.toLowerCase()}"
        value={held.id}
        onchange={(event) => choose((event.currentTarget as HTMLSelectElement).value)}
      >
        {#each themes.of(kind) as t (t.id)}
          <option value={t.id}>{t.id.startsWith("own-") ? `${t.name}, yours` : t.name}</option>
        {/each}
      </select>
      <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (editingId = held.id)}>Edit</button>
      <div class="tf-thm-chips" aria-hidden="true">
        {#each CHIPS as role (role)}
          <i title={ROLE_NAMES[role]} {@attach cssVars(() => ({ background: held.syntax[role] ?? INK[kind] }))}></i>
        {/each}
      </div>
    </div>
  {/each}
</div>
<!-- Not keyed by the theme: the preview's code stays drawn while another theme is shown, and only its colours change. -->
<TfThemeEditor theme={editing} {themes} {drawnSide} {render} onduplicate={duplicate} ondelete={remove} />
