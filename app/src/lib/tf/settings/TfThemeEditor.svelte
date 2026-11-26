<!--
  Settings › Appearance › Code colours' editor (ADR-0051): one theme, the one last chosen or edited. Its name and where it came from, its
  actions, the role list written in the theme's own colours, the diff's four colours, and beside them the preview made
  of the app's real code areas (`TfThemePreview.svelte`), with how a diff's unchanged lines draw and whether comments
  slant under it.

  A built-in is read-only: its swatches are not buttons and its first action is *Duplicate and edit*. Each row gives
  the role's lowest contrast over every surface code is drawn on (`theme/contrast.ts`); under 4.5:1 the row says on
  which, and offers the fix that moves the colour's lightness alone.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import type { RenderPort } from "../../render/port";
  import { ROLE_NAMES, ROLES, type Role } from "../../render/roles";
  import { undoable } from "../../shell/toasts.svelte";
  import { layers } from "../layers.svelte";
  import { scopeValues, UNCHANGED, type Themes, type Unchanged } from "../themes.svelte";
  import { ratioText } from "../theme/colour";
  import { BUILTINS } from "../theme/builtins";
  import { diffDefault, fixedRole, lowSentence, roleContrast } from "../theme/contrast";
  import { cssVars } from "../theme/paint";
  import { DIFF_KEYS, exportTheme, hasApp, LONGEST_NAME, themeName, type DiffKey, type Theme, type ThemeKind } from "../theme/themes";
  import TfThemePreview from "./TfThemePreview.svelte";
  import TfThemeSwatch from "./TfThemeSwatch.svelte";

  let {
    theme,
    themes,
    drawnSide,
    render,
    onduplicate,
    ondelete,
    onfixcopy,
  }: {
    theme: Theme;
    themes: Themes;
    drawnSide: ThemeKind;
    render: RenderPort;
    onduplicate: (theme: Theme) => void;
    ondelete: (theme: Theme) => void;
    /** A built-in's fix: a copy of it with these colours, where it drew (Björn, 2026-11-22: built-ins keep their colours, and he lightens them). */
    onfixcopy: (theme: Theme, syntax: Partial<Theme["syntax"]>) => void;
  } = $props();

  /** A short sample of each role, drawn in the role's own class. */
  const SAMPLE: Record<Role, string> = {
    com: "// retry later",
    kw: "while return",
    str: '"retry {}"',
    esc: "\\n {} /ab+c/",
    num: "0 0x1F 250",
    con: "MAX_TRIES null",
    fn: "settle() send()",
    ty: "Receipt Batch",
    prop: "batch.id",
    key: "image: limit:",
    tag: "<dependency>",
    attr: "@Override",
    var: "batch tries",
    op: "= ++ < &&",
    punct: "( ) { } ; ,",
    head: "## Running it",
    link: "[the notes](retry.md)",
  };
  const DIFF_NAMES: Record<DiffKey, string> = { added: "Added line", removed: "Removed line", addedWord: "Added word", removedWord: "Removed word" };
  const UNCHANGED_NAMES: Record<Unchanged, string> = { full: "Full colour", quiet: "Quiet", grey: "Grey" };

  const own = $derived(theme.id.startsWith("own-"));
  /** The theme this one came from, where knobas still has it: what *Reset to ⟨from⟩'s* goes back to. */
  const source = $derived(theme.from === undefined || theme.from === theme.name ? undefined : [...BUILTINS, ...themes.own].find((t) => t.name === theme.from && t.id !== theme.id));
  const tints = $derived({ ...(theme.diff?.added ? { added: theme.diff.added } : {}), ...(theme.diff?.removed ? { removed: theme.diff.removed } : {}) });
  const fixWord = $derived(theme.kind === "light" ? "Darken" : "Lighten");
  /** The window theme drawn for this theme's side: the surfaces its code is checked and shown on (ADR-0055). */
  const windowOf = $derived(themes.appDrawn(theme.kind));
  const surfaces = $derived(themes.colours(theme.kind));
  const DIFF_DEFAULT = $derived(diffDefault(surfaces));

  function setRole(role: Role, next: string | undefined, preview = false): void {
    themes.update(
      theme.id,
      (t) => {
        const syntax = { ...t.syntax };
        if (next === undefined) delete syntax[role];
        else syntax[role] = next;
        return { ...t, syntax };
      },
      preview,
    );
  }

  function setDiff(key: DiffKey, next: string | undefined, preview = false): void {
    themes.update(
      theme.id,
      (t) => {
        const diff = { ...t.diff };
        if (next === undefined) delete diff[key];
        else diff[key] = next;
        const { diff: _gone, ...rest } = t;
        return Object.keys(diff).length > 0 ? { ...rest, diff } : rest;
      },
      preview,
    );
  }

  /** Every role under 4.5:1, fixed by lightness. */
  const lowFixes = $derived(Object.fromEntries(ROLES.flatMap((role) => {
    const next = fixedRole(surfaces, theme.kind, role, theme.syntax[role], tints);
    return next === null ? [] : [[role, next]];
  })) as Partial<Theme["syntax"]>);

  function fixAllRoles(): void {
    const fixes = lowFixes;
    if (Object.keys(fixes).length === 0) return;
    if (!own) return onfixcopy(theme, fixes);
    const was = { ...theme.syntax };
    themes.update(theme.id, (t) => ({ ...t, syntax: { ...t.syntax, ...fixes } }));
    undoable(`${Object.keys(fixes).length === 1 ? "1 colour" : `${Object.keys(fixes).length} colours`} ${fixWord === "Darken" ? "darkened" : "lightened"} to 4.5:1.`, () => themes.update(theme.id, (t) => ({ ...t, syntax: was })), { subject: `theme-fix-${theme.id}` });
  }

  function fixRole(role: Role): void {
    const was = theme.syntax[role];
    const next = fixedRole(surfaces, theme.kind, role, was, tints);
    if (next === null) return;
    if (!own) return onfixcopy(theme, { [role]: next });
    setRole(role, next);
    undoable(`${ROLE_NAMES[role]} ${fixWord === "Darken" ? "darkened" : "lightened"} to 4.5:1.`, () => setRole(role, was), { subject: `theme-fix-${theme.id}-${role}` });
  }

  // ---------- rename ----------
  let renaming = $state(false);
  let typedName = $state("");
  let nameField = $state<HTMLInputElement | null>(null);

  function startRename(): void {
    typedName = theme.name;
    renaming = true;
    void tick().then(() => nameField?.select());
  }
  function endRename(save: boolean): void {
    if (save) {
      const taken = new Set([...BUILTINS, ...themes.own].filter((t) => t.id !== theme.id).map((t) => t.name));
      const name = themeName(typedName);
      if (!taken.has(name)) themes.update(theme.id, (t) => ({ ...t, name }));
    }
    renaming = false;
  }
  // Another theme in the editor ends a rename of the one before.
  $effect.pre(() => {
    void theme.id;
    untrack(() => (renaming = false));
  });
  $effect(() => {
    if (!renaming) return;
    return untrack(() => layers.add({ kind: "panel", close: () => endRename(false) }));
  });

  // ---------- export ----------
  function exportIt(): void {
    const { fileName, text } = exportTheme(theme);
    const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = fileName;
    link.hidden = true;
    document.body.append(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
</script>

<div class="tf-thm-ed">
  <div class="tf-thm-edh">
    {#if renaming}
      <input
        class="tf-stg-in tf-thm-name"
        aria-label="Name"
        maxlength={LONGEST_NAME}
        bind:value={typedName}
        bind:this={nameField}
        onkeydown={(event) => {
          if (event.key === "Enter") endRename(true);
          else if (event.key === "Escape") {
            event.stopPropagation();
            endRename(false);
          }
        }}
        onblur={() => endRename(true)}
      />
    {:else}
      <h2>{theme.name}</h2>
    {/if}
    <span class="tf-lab">{own ? (theme.from && theme.from !== theme.name ? `yours, from ${theme.from}` : "yours") : "comes with knobas"}</span>
    <div class="tf-trail">
      {#if Object.keys(lowFixes).length > 0}
        <button type="button" class="tf-btn tf-sm" onclick={fixAllRoles}>Fix all</button>
      {/if}
      {#if own}
        <button type="button" class="tf-btn tf-sm" onclick={startRename}>Rename…</button>
        <button type="button" class="tf-btn tf-sm" onclick={() => onduplicate(theme)}>Duplicate</button>
        <button type="button" class="tf-btn tf-sm" onclick={exportIt}>Export…</button>
        <button type="button" class="tf-btn tf-sm" onclick={() => ondelete(theme)}>Delete…</button>
      {:else}
        <button type="button" class="tf-btn tf-sm tf-primary" onclick={() => onduplicate(theme)}>Duplicate and edit</button>
        <button type="button" class="tf-btn tf-sm" onclick={exportIt}>Export…</button>
      {/if}
    </div>
  </div>
  {#if hasApp(theme)}
    {#if themes.appSlot(theme.kind) !== theme.id}
      <div class="tf-thm-app">
        <span>{theme.name} has colours for the window too.</span>
        <button
          type="button"
          class="tf-btn tf-sm"
          onclick={() => {
            themes.chooseApp(theme.id);
            themes.follow(theme.kind);
          }}>Use for the app too</button
        >
      </div>
    {/if}
  {:else}
    <p class="tf-thm-app tf-dim">It has no window colours. Import its file again to read them.</p>
  {/if}
  <div
    class="tf-thm-roles"
    {@attach cssVars(() => (theme.kind !== drawnSide ? scopeValues(windowOf, theme, surfaces) : { "--hl-com-style": theme.italicComments === false ? "normal" : "italic" }))}
  >
    <div class="tf-thm-grp">Code<small>lowest contrast on any surface of {windowOf.name}</small></div>
    {#each ROLES as role (role)}
      {@const value = theme.syntax[role]}
      {@const c = roleContrast(surfaces, role, value, tints)}
      <div class="tf-thm-role" class:tf-low={c.low} data-role={role}>
        <TfThemeSwatch
          label={ROLE_NAMES[role]}
          {value}
          fallback={surfaces.ink}
          editable={own}
          resetTo={source ? source.syntax[role] : null}
          resetName={source?.name ?? ""}
          onchange={(next, preview) => setRole(role, next, preview)}
        />
        <span>{ROLE_NAMES[role]}</span>
        <code class="tf-hl-{role}" {@attach cssVars(() => ({ "--hl": value ?? "var(--ink)" }))}>{SAMPLE[role]}</code>
        <span class="tf-thm-hex">{value ?? "ink"}</span>
        <span class="tf-thm-cr">{ratioText(c.ratio)}</span>
        {#if c.low}
          <div class="tf-thm-why">
            {lowSentence(c.ratio, c.on)}
            <button type="button" class="tf-btn tf-sm" onclick={() => fixRole(role)}>{fixWord} to 4.5:1</button>
          </div>
        {/if}
      </div>
    {/each}
    <div class="tf-thm-grp">Diff</div>
    {#each DIFF_KEYS as key (key)}
      {@const value = theme.diff?.[key]}
      <div class="tf-thm-role" data-diff={key}>
        <TfThemeSwatch
          label={DIFF_NAMES[key]}
          {value}
          fallback={DIFF_DEFAULT[key]}
          editable={own}
          inkable={false}
          resetTo={source ? (source.diff?.[key] ?? undefined) : null}
          resetName={source?.name ?? ""}
          onchange={(next, preview) => setDiff(key, next, preview)}
        />
        <span>{DIFF_NAMES[key]}</span>
        <code></code>
        <span class="tf-thm-hex">{value ?? DIFF_DEFAULT[key]}</span>
        <span class="tf-thm-cr"></span>
      </div>
    {/each}
  </div>
  <div class="tf-thm-side">
    <TfThemePreview {theme} {drawnSide} {render} windowTheme={windowOf} colours={surfaces} />
    <div class="tf-thm-foot">
      <span>Unchanged lines in a diff</span>
      <div class="tf-seg" role="group" aria-label="Unchanged lines in a diff">
        {#each UNCHANGED as key (key)}
          <button type="button" aria-pressed={themes.unchanged === key} onclick={() => themes.setUnchanged(key)}>{UNCHANGED_NAMES[key]}</button>
        {/each}
      </div>
      <label class="tf-thm-italic"
        ><input
          type="checkbox"
          checked={theme.italicComments !== false}
          disabled={!own}
          onchange={(event) => {
            const on = (event.currentTarget as HTMLInputElement).checked;
            themes.update(theme.id, (t) => {
              const { italicComments: _was, ...rest } = t;
              return on ? rest : { ...rest, italicComments: false };
            });
          }}
        /> Comments in italics</label
      >
    </div>
  </div>
</div>
