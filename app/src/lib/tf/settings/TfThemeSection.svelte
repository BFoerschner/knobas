<!--
  Settings › Appearance › Theme (ADR-0055, `docs/decisions/2026-11-21-a-theme-colours-the-whole-window.md`, 3.9 to 3.11):
  the colours of the whole window. Top to bottom: the page's sentence and *Import…*, the two window slots (*While
  light*, *While dark*), each with eight chips and a line on what code does beside it, then the editor of the theme
  last chosen, imported or edited (`TfAppThemeEditor.svelte`), and at the foot *Put knobas's themes back*.

  A theme just imported shows a band over its role list that says what the file held and where to use it: *The app
  and code*, *The app only* (code is set apart first, so it stays as it is), *Code only*. A theme whose text falls
  under 3:1 asks first, in a dialog in the colours the window has now: *Cancel*, *Use as it is* (which draws it for
  15 s unless kept, `TfThemeKeep.svelte`) and *Fix and use*. Everything here is this computer's (`themes.svelte.ts`).
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import { push, undoable } from "../../shell/toasts.svelte";
  import { keyHint } from "../../shell/platform";
  import { layers } from "../layers.svelte";
  import { fixAll, unreadable } from "../theme/app-check";
  import { CHIPS } from "../theme/app-roles";
  import { cssVars } from "../theme/paint";
  import { hasApp, hasCode, type Theme, type ThemeKind } from "../theme/themes";
  import { importText } from "../theme-import";
  import { RESET_HINT, putKnobasBack } from "../theme-reset";
  import { theme as windowTheme, type Theme as WindowTheme } from "../theme.svelte";
  import { themes as appThemes, type Themes } from "../themes.svelte";
  import TfAppThemeEditor from "./TfAppThemeEditor.svelte";
  import TfDialog from "./TfDialog.svelte";

  let {
    themes = appThemes,
    mode = windowTheme,
    prefersDark = () => typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia("(prefers-color-scheme: dark)").matches,
  }: { themes?: Themes; mode?: WindowTheme; prefersDark?: () => boolean } = $props();

  const SLOTS: readonly (readonly [ThemeKind, string])[] = [
    ["light", "While light"],
    ["dark", "While dark"],
  ];

  let systemDark = $state(untrack(() => prefersDark()));
  onMount(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const heard = () => (systemDark = query.matches);
    query.addEventListener("change", heard);
    return () => query.removeEventListener("change", heard);
  });
  const drawnSide = $derived<ThemeKind>(mode.choice === "system" ? (systemDark ? "dark" : "light") : mode.choice);

  /** The theme in the editor: the one last chosen, imported or edited, at first the one the window draws now. */
  let editingId = $state(untrack(() => themes.appDrawn(drawnSide).id));
  const editing = $derived(themes.find(editingId) ?? themes.appDrawn(drawnSide));
  /** The theme just imported, whose band asks where to use it; it goes once a button is pressed or another is shown. */
  let justImported = $state<string | null>(null);
  const showBand = $derived(justImported !== null && justImported === editing.id);

  const label = (t: Theme) => (t.id.startsWith("own-") ? `${t.name}, yours` : t.name);

  // ---------- choosing, and asking first ----------
  /** The theme a dialog asks about before it is used, and what using it does. */
  let asking = $state<{ theme: Theme; then: () => void } | null>(null);
  const askText = $derived.by(() => {
    if (!asking) return "";
    const f = unreadable(themes.coloursOf(asking.theme))[0];
    return f ? `Its ${f.key === "ink" ? "text" : f.key === "onPrimary" ? "text on a primary button" : f.key === "onFill" ? "text on what is selected" : "text on the card"} is ${(Math.floor(f.ratio * 10) / 10).toFixed(1)}:1 on ${f.on}. Under 3:1, words are hard to make out.` : "";
  });

  /** Uses a theme for the window, asking first where it cannot be read; `then` is what else the press does. */
  function useForWindow(t: Theme, then: () => void = () => {}): void {
    if (themes.asksFirst(t)) {
      asking = { theme: t, then };
      return;
    }
    themes.chooseApp(t.id);
    then();
  }

  function chooseSlot(select: HTMLSelectElement): void {
    const t = themes.find(select.value);
    if (!t) return;
    editingId = t.id;
    useForWindow(t);
    // A theme that asks first is not in the slot yet: the select shows what the slot draws until it is.
    select.value = themes.appDrawn(t.kind).id;
  }

  function fixAndUse(): void {
    if (!asking) return;
    const { theme: t, then } = asking;
    asking = null;
    const was = t.app ?? {};
    themes.update(t.id, (x) => ({ ...x, app: fixAll(x.kind, x.app ?? {}) }));
    themes.flush();
    themes.chooseApp(t.id);
    then();
    undoable(`Fixed ${t.name}'s colours and used it.`, () => (themes.update(t.id, (x) => ({ ...x, app: was })), themes.flush()), { subject: `app-theme-fix-${t.id}` });
  }

  function useAsIs(): void {
    if (!asking) return;
    const { theme: t, then } = asking;
    asking = null;
    themes.useAsIs(t.id);
    then();
  }

  // ---------- the band ----------
  function appAndCode(t: Theme): void {
    useForWindow(t, () => themes.follow(t.kind));
    justImported = null;
  }
  function appOnly(t: Theme): void {
    // Code stays as it is: set apart first, so it never follows the new theme for a frame.
    if (themes.follows(t.kind)) themes.setApart(t.kind);
    useForWindow(t);
    justImported = null;
  }
  function codeOnly(t: Theme): void {
    themes.choose(t.id);
    justImported = null;
  }
  function useIt(t: Theme): void {
    if (hasApp(t)) appAndCode(t);
    else codeOnly(t);
  }

  // ---------- copies, deletes, import ----------
  function duplicate(of: Theme): void {
    const { id: _id, ...rest } = of;
    const copy = themes.add({ ...rest, app: { ...(of.app ?? {}) }, name: `${of.name} copy`, from: of.name });
    if (copy === null) return void push({ text: "knobas keeps 64 themes of your own. Delete one to make another." });
    // The copy goes where the original drew.
    if (themes.appSlot(of.kind) === of.id || themes.appDrawn(of.kind).id === of.id) themes.chooseApp(copy.id);
    editingId = copy.id;
  }

  function remove(gone: Theme): void {
    const undo = themes.remove(gone.id);
    if (undo === null) return;
    if (editingId === gone.id) editingId = themes.appDrawn(gone.kind).id;
    undoable(`Deleted ${gone.name}.`, () => {
      undo();
      editingId = gone.id;
    });
  }

  let picker = $state<HTMLInputElement | null>(null);
  async function imported(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    const added = importText(themes, await file.text());
    if (!added) return;
    editingId = added[0]!.id;
    justImported = added[0]!.id;
  }
</script>

<header class="tf-stg-h tf-thm-top tf-apt-top">
  <div>
    <h1>Theme</h1>
    <p>The colours of the whole window. One theme draws while the window is light, one while it is dark. Kept on this computer.</p>
    <p class="tf-stg-note">From VS Code, run "Developer: Generate Color Theme From Current Settings", save the file and import it here.</p>
  </div>
  <div class="tf-acts">
    <button type="button" class="tf-btn" onclick={() => picker?.click()}>Import…</button>
    <input type="file" accept=".json,.jsonc,.yaml,.yml,application/json" hidden bind:this={picker} onchange={imported} aria-label="Theme file" />
  </div>
</header>
{#if themes.startedDefault}
  <div class="tf-apt-started" role="status">
    <span>Started with knobas's own themes. What you chose draws again on the next start.</span>
    <button type="button" class="tf-btn tf-sm" onclick={() => putKnobasBack(themes)}>Put knobas's themes back</button>
  </div>
{/if}
<div class="tf-thm-slots">
  {#each SLOTS as [kind, slotLabel] (kind)}
    {@const held = themes.appDrawn(kind)}
    {@const colours = themes.colours(kind)}
    {@const code = themes.drawn(kind)}
    <div class="tf-thm-slot" class:tf-cur={kind === drawnSide} data-slot={kind}>
      <h3>{slotLabel}</h3>
      <select class="tf-thm-sel" aria-label="Window theme {slotLabel.toLowerCase()}" value={held.id} onchange={(event) => chooseSlot(event.currentTarget as HTMLSelectElement)}>
        {#each themes.appOf(kind) as t (t.id)}
          <option value={t.id}>{label(t)}</option>
        {/each}
      </select>
      <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (editingId = held.id)}>Edit</button>
      <div class="tf-thm-chips tf-apt-chips" aria-hidden="true">
        {#each CHIPS as key (key)}
          <i {@attach cssVars(() => ({ background: colours[key as keyof typeof colours] ?? colours.sheet }))}></i>
        {/each}
      </div>
      <div class="tf-apt-code">
        {#if themes.follows(kind)}
          <span>{hasCode(held) ? "Code colours follow this theme." : `Code colours follow this theme, which has none, so ${code.name} draws.`}</span>
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => themes.setApart(kind)}>Set apart</button>
        {:else}
          <span>Code colours are {code.name}.</span>
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => themes.follow(kind)}>Follow this theme</button>
        {/if}
      </div>
    </div>
  {/each}
</div>
<!-- Not keyed by the theme: the editor stays drawn while another theme is shown, and only its colours change. -->
<TfAppThemeEditor theme={editing} {themes} {drawnSide} onduplicate={duplicate} ondelete={remove}>
  {#snippet band()}
    {#if showBand}
      {@const both = hasApp(editing) && hasCode(editing)}
      <div class="tf-apt-use" role="group" aria-label="Use {editing.name}">
        {#if both}
          <span>{editing.name} has colours for the window and for code. Use it while {editing.kind} for</span>
          <button type="button" class="tf-btn tf-sm tf-primary" onclick={() => appAndCode(editing)}>The app and code</button>
          <button type="button" class="tf-btn tf-sm" onclick={() => appOnly(editing)}>The app only</button>
          <button type="button" class="tf-btn tf-sm" onclick={() => codeOnly(editing)}>Code only</button>
        {:else}
          <span>{editing.name} has colours for {hasApp(editing) ? "the window" : "code"}.</span>
          <button type="button" class="tf-btn tf-sm tf-primary" onclick={() => useIt(editing)}>Use it while {editing.kind}</button>
        {/if}
      </div>
    {/if}
  {/snippet}
</TfAppThemeEditor>
<div class="tf-apt-foot">
  <button type="button" class="tf-btn tf-ghost" onclick={() => putKnobasBack(themes)}>Put knobas's themes back</button>
  <span class="tf-dim">Or press {keyHint(RESET_HINT)} anywhere.</span>
</div>
{#if asking}
  <TfDialog title="{asking.theme.name} would be hard to read" {layers} onclose={() => (asking = null)}>
    {#snippet body()}
      <p>{askText}</p>
      <p class="tf-dlg-note">
        Fixing changes the colours under their mark and keeps their hues. Used as it is, knobas goes back to {themes.appDrawn(asking!.theme.kind).name} after 15 seconds unless you keep it.
        {keyHint(RESET_HINT)} puts knobas's own themes back at any time.
      </p>
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (asking = null)}>Cancel</button>
      <button type="button" class="tf-btn" onclick={useAsIs}>Use as it is</button>
      <button type="button" class="tf-btn tf-primary" onclick={fixAndUse}>Fix and use</button>
    {/snippet}
  </TfDialog>
{/if}
