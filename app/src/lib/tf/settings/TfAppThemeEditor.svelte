<!--
  Settings › Appearance › Theme's editor (ADR-0055, the decisions' 3.8 and 3.11): one theme's window colours. Its name
  and where it came from, its actions (Code colours' own), then the role list written in the theme's own colours (22
  rows under seven headings, a ground and its text in one row with one figure) and the preview beside it.

  A row's swatch sets its key; *Auto* leaves it out, so it is derived again and says *auto*. A row's figure is its
  lowest contrast against what it needs; under it the row says the pair and offers the fix, which moves lightness
  alone, and for a filled pair the ground. Under the States, how far apart the states are for a colour-blind reader,
  and on a theme of one's own *Tell them apart*. Under a fold, the 19 colours knobas works out from the rows.
-->
<script lang="ts">
  import { tick, untrack, type Snippet } from "svelte";

  import { undoable } from "../../shell/toasts.svelte";
  import { layers } from "../layers.svelte";
  import { apartLine, closestStates, failures, fixAll, fixRow, fixWords, lowSentence, rowFigure, tellApart, APART } from "../theme/app-check";
  import { colourBlind, confusions } from "../colour-blind.svelte";
  import { derive } from "../theme/app-derive";
  import { FOLD, HEADINGS, NAME, type AppKey, type Row } from "../theme/app-roles";
  import { BUILTINS } from "../theme/builtins";
  import { ratioText } from "../theme/colour";
  import { cssVars, saveThemeFile } from "../theme/paint";
  import { LONGEST_NAME, themeName, type Theme, type ThemeKind } from "../theme/themes";
  import { scopeValues, type Themes } from "../themes.svelte";
  import TfAppThemePreview from "./TfAppThemePreview.svelte";
  import TfThemeSwatch from "./TfThemeSwatch.svelte";

  let {
    theme,
    themes,
    drawnSide,
    onduplicate,
    ondelete,
    band,
  }: {
    theme: Theme;
    themes: Themes;
    drawnSide: ThemeKind;
    onduplicate: (theme: Theme) => void;
    ondelete: (theme: Theme) => void;
    /** What shows over the role list: the import's band (`TfThemeSection.svelte`). */
    band?: Snippet;
  } = $props();

  const own = $derived(theme.id.startsWith("own-"));
  const set = $derived(theme.app ?? {});
  const result = $derived(derive(theme.kind, set));
  const colours = $derived(result.colours);
  /** The code drawn in the preview: the theme's own where it has them, else those drawn for its side. */
  const code = $derived(Object.keys(theme.syntax).length > 0 ? theme : themes.drawn(theme.kind));
  /** The theme this one came from, where knobas still has it: what *Reset to ⟨from⟩'s* goes back to. */
  const source = $derived(theme.from === undefined || theme.from === theme.name ? undefined : [...BUILTINS, ...themes.own].find((t) => t.name === theme.from && t.id !== theme.id));
  const sourceColours = $derived(source ? derive(source.kind, source.app ?? {}) : null);

  let folded = $state(true);
  let list = $state<HTMLElement | null>(null);

  /** A colour picked in the preview: its row scrolled to and, on a theme of one's own, its swatch's panel open. */
  async function pickKey(key: AppKey): Promise<void> {
    if (FOLD.includes(key)) folded = false;
    await tick();
    const swatch = list?.querySelector<HTMLElement>(`[aria-label="Change ${NAME[key]}"], [role="img"][aria-label^="${NAME[key]}:"]`);
    swatch?.scrollIntoView?.({ block: "nearest" });
    if (swatch instanceof HTMLButtonElement) swatch.click();
    swatch?.focus();
  }

  function setKey(key: AppKey, next: string | undefined, preview = false): void {
    themes.update(
      theme.id,
      (t) => {
        const app = { ...t.app };
        if (next === undefined) delete app[key];
        else app[key] = next;
        return { ...t, app };
      },
      preview,
    );
  }

  /**
   * A selected row's quick picks (Björn, 2026-11-22): knobas's ink plate (the pair left out, so derived), the theme's
   * accent (its words then the more readable of panel and text), or the row its file drew, where it had one.
   */
  function pickRow(which: "ink" | "accent" | "own"): void {
    themes.update(theme.id, (t) => {
      const app = { ...t.app };
      delete app.selRow;
      delete app.onSelRow;
      if (which === "accent") app.selRow = derive(t.kind, t.app ?? {}).colours.primary;
      if (which === "own" && t.offers?.selRow) {
        app.selRow = t.offers.selRow;
        if (t.offers.onSelRow) app.onSelRow = t.offers.onSelRow;
      }
      return { ...t, app };
    });
  }

  function setMany(next: Partial<Record<AppKey, string>>, words: string): void {
    const was = { ...set };
    const id = theme.id;
    themes.update(id, (t) => ({ ...t, app: next }));
    themes.flush();
    undoable(words, () => (themes.update(id, (t) => ({ ...t, app: was })), themes.flush()), { subject: `app-theme-fix-${id}` });
  }

  function fixIt(row: Row): void {
    const f = rowFigure(colours, row.keys);
    if (!f) return;
    // *Darken the button to 4.5:1* reads, once done, *Primary button: the button darkened to 4.5:1.*
    const done = fixWords(theme.kind, colours, f).replace(/^Darken/, "darkened").replace(/^Lighten/, "lightened");
    setMany(fixRow(theme.kind, set, row.keys), done.startsWith("darkened the") || done.startsWith("lightened the") ? `${row.name}: ${done.replace(/^(\w+) (the \w+)/, "$2 $1")}.` : `${row.name} ${done}.`);
  }

  function apart(): void {
    const got = tellApart(colours);
    const next = { ...set, ok: got.ok, warn: got.warn, err: got.err };
    delete next.okBg;
    delete next.warnBg;
    delete next.errBg;
    const running = got.info !== colours.info;
    if (running) {
      next.info = got.info;
      delete next.infoBg;
    }
    setMany(next, `${running ? "Passed, Attention, Failed and Running" : "Passed, Attention and Failed"} moved ${got.reached.toFixed(2)} apart.`);
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
  $effect.pre(() => {
    void theme.id;
    untrack(() => (renaming = false));
  });
  $effect(() => {
    if (!renaming) return;
    return untrack(() => layers.add({ kind: "panel", close: () => endRename(false) }));
  });

  const exampleOf = (row: Row): Record<string, string> => {
    const [a, b] = row.keys.map((k) => colours[k as keyof typeof colours] ?? "");
    switch (row.example) {
      case "text":
        return row.keys[0] === "muted" ? { background: colours.field, color: a! } : { background: colours.sheet, color: a! };
      case "line":
        return { "border-top-color": a! };
      case "chip":
        // A state on its tint.
        return { background: b!, color: a! };
      case "button":
      case "segment":
      case "selrow":
      case "card":
        // A ground and the words on it.
        return { background: a!, color: b! };
      case "unknown":
        return { background: a!, color: colours.muted };
      case "ring":
        return { "outline-color": a!, background: colours.sheet };
      case "link":
        return { background: colours.sheet, color: a! };
      case "mark":
        return { background: colours.sheet, color: colours.ink };
      default:
        return {};
    }
  };
  const isAuto = (row: Row) => row.keys.every((k) => (k === "selection" ? set.selection === undefined : result.derived.has(k)));
</script>

<div class="tf-thm-ed tf-apt-ed">
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
      {#if own && failures(colours).length > 0}
        <button type="button" class="tf-btn tf-sm" onclick={() => setMany(fixAll(theme.kind, set), `${failures(colours).length === 1 ? "1 colour" : `${failures(colours).length} colours`} fixed by lightness.`)}>Fix all</button>
      {/if}
      {#if own}
        <button type="button" class="tf-btn tf-sm" onclick={startRename}>Rename…</button>
        <button type="button" class="tf-btn tf-sm" onclick={() => onduplicate(theme)}>Duplicate</button>
        <button type="button" class="tf-btn tf-sm" onclick={() => saveThemeFile(theme)}>Export…</button>
        <button type="button" class="tf-btn tf-sm" onclick={() => ondelete(theme)}>Delete…</button>
      {:else}
        <button type="button" class="tf-btn tf-sm tf-primary" onclick={() => onduplicate(theme)}>Duplicate and edit</button>
        <button type="button" class="tf-btn tf-sm" onclick={() => saveThemeFile(theme)}>Export…</button>
      {/if}
    </div>
  </div>
  {#if band}{@render band()}{/if}
  <div class="tf-thm-roles tf-apt-roles" bind:this={list} {@attach cssVars(() => scopeValues(theme, code, colours))}>
    {#each HEADINGS as heading (heading.name)}
      <div class="tf-thm-grp">{heading.name}{#if heading.note}<small>{heading.note}</small>{/if}</div>
      {#each heading.rows as row (row.name)}
        {@const fig = rowFigure(colours, row.keys)}
        {@const low = fig !== null && fig.ratio < fig.want}
        <div class="tf-thm-role tf-apt-role" class:tf-low={low} data-row={row.keys[0]}>
          <span class="tf-apt-sws">
            {#each row.keys as key (key)}
              <TfThemeSwatch
                label={NAME[key]}
                value={set[key]}
                fallback={colours[key as keyof typeof colours] ?? colours.sheet}
                editable={own}
                leave="Auto"
                resetTo={sourceColours ? (source?.app?.[key] ?? undefined) : null}
                resetName={source?.name ?? ""}
                onchange={(next, preview) => setKey(key, next, preview)}
              />
            {/each}
          </span>
          <span>{row.name}</span>
          <span class="tf-apt-eg">
            {#if row.example === "words"}
              <span>{row.words}</span>
            {:else if row.example === "line"}
              <i class="tf-apt-line" {@attach cssVars(() => exampleOf(row))}></i><span>{row.words}</span>
            {:else if row.example === "ring"}
              <i class="tf-apt-ring" {@attach cssVars(() => exampleOf(row))}></i><span>{row.words}</span>
            {:else if row.example === "button"}
              <span class="tf-apt-plate tf-apt-btn" {@attach cssVars(() => exampleOf(row))}>{row.words}</span>
            {:else if row.example === "segment"}
              <span class="tf-apt-plate tf-apt-btn" {@attach cssVars(() => exampleOf(row))}>{row.words}</span><span>List</span>
            {:else if row.example === "selrow"}
              <span class="tf-apt-plate tf-apt-btn" {@attach cssVars(() => exampleOf(row))}>{row.words}</span><span>Mine</span>
            {:else if row.example === "chip" || row.example === "unknown"}
              <span class="tf-chip" {@attach cssVars(() => exampleOf(row))}>{row.words}</span>
            {:else if row.example === "card"}
              <span class="tf-apt-plate tf-apt-cardeg" class:tf-apt-strong={row.keys[0] === "card"} {@attach cssVars(() => exampleOf(row))}>{row.words}</span>
            {:else if row.example === "mark"}
              <span class="tf-apt-plate" {@attach cssVars(() => exampleOf(row))}>re<mark class="tf-apt-mark">try</mark> queue</span>
            {:else if row.example === "selection"}
              {#if colours.selection}
                <span class="tf-apt-plate"><span class="tf-apt-sel">{row.words}</span> words</span>
              {:else}
                <span>The system's own</span>
              {/if}
            {:else}
              <span class="tf-apt-plate" {@attach cssVars(() => exampleOf(row))}>{row.words}</span>
            {/if}
          </span>
          <span class="tf-thm-hex tf-apt-hex">{row.keys[0] === "selection" ? (set.selection ?? "") : colours[row.keys[0] as keyof typeof colours]}{#if isAuto(row)}<i class="tf-apt-auto">auto</i>{/if}</span>
          <span class="tf-thm-cr">{fig ? ratioText(fig.ratio) : ""}</span>
          {#if own && row.example === "selrow"}
            <div class="tf-apt-picks" role="group" aria-label="Selected row's colours">
              <button type="button" class="tf-btn tf-sm" aria-pressed={set.selRow === undefined} onclick={() => pickRow("ink")}>knobas (ink)</button>
              <button type="button" class="tf-btn tf-sm" aria-pressed={set.selRow !== undefined && set.selRow === colours.primary} onclick={() => pickRow("accent")}>The theme's accent</button>
              {#if theme.offers?.selRow}
                <button type="button" class="tf-btn tf-sm" aria-pressed={set.selRow === theme.offers.selRow} onclick={() => pickRow("own")}>The theme's own</button>
              {/if}
            </div>
          {/if}
          {#if low && fig}
            <div class="tf-thm-why">
              {lowSentence(fig)}
              {#if own}<button type="button" class="tf-btn tf-sm" onclick={() => fixIt(row)}>{fixWords(theme.kind, colours, fig)}</button>{/if}
            </div>
          {/if}
        </div>
      {/each}
      {#if heading.name === "States"}
        {#if colourBlind.on}
          {#each confusions(colours) as pair (`${pair.a}-${pair.b}-${pair.vision}`)}
            <div class="tf-apt-told tf-apt-cb" role="note">{pair.sentence}</div>
          {/each}
        {/if}
        <div class="tf-apt-told">
          <span>{apartLine(colours)}</span>
          {#if own && closestStates(colours).delta < APART}<button type="button" class="tf-btn tf-sm" onclick={apart}>Tell them apart</button>{/if}
        </div>
      {/if}
    {/each}
    <button type="button" class="tf-apt-more" aria-expanded={!folded} onclick={() => (folded = !folded)}>{folded ? "▸" : "▾"} Fine tuning: 19 colours knobas works out from the ones above</button>
    {#if !folded}
      {#each FOLD as key (key)}
        <div class="tf-thm-role tf-apt-role tf-apt-fold" data-row={key}>
          <span class="tf-apt-sws">
            <TfThemeSwatch
              label={NAME[key]}
              value={set[key]}
              fallback={colours[key as keyof typeof colours] ?? colours.sheet}
              editable={own}
              leave="Auto"
              resetTo={sourceColours ? (source?.app?.[key] ?? undefined) : null}
              resetName={source?.name ?? ""}
              onchange={(next, preview) => setKey(key, next, preview)}
            />
          </span>
          <span class="tf-apt-foldname">{NAME[key]}</span>
          <span></span>
          <span class="tf-thm-hex tf-apt-hex">{colours[key as keyof typeof colours]}{#if result.derived.has(key)}<i class="tf-apt-auto">auto</i>{/if}</span>
          <span class="tf-thm-cr"></span>
        </div>
      {/each}
    {/if}
  </div>
  <div class="tf-thm-side">
    <TfAppThemePreview
      {theme}
      {code}
      {colours}
      onpick={(key) => void pickKey(key)}
      note={theme.kind !== drawnSide ? `Shown as it draws while the window is ${theme.kind}.` : "Made of knobas's own parts. Every colour here is the theme's."}
    />
  </div>
</div>
