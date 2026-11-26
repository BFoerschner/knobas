<!--
  Settings › Fonts: one role's face and size (2026-11-18, `fonts.svelte.ts`; it was *Code font* alone, 2026-11-16).

  The field is a combobox over the families installed on this machine (`installed_fonts`, read once a run by the
  backend), the role's own kind first under a heading each: monospace for Code, serif for Reading, those that draw
  narrow for Headings (the rest marked *normal width only*), sans serif for the Interface. A name typed that is not
  listed is taken as it is, since a font can be installed where the read does not look. A click, a key typed or `↓`
  opens the list, focus alone does not: Settings puts the focus in a section's first field as it opens. The option the
  keys or the pointer are on draws in the specimen before it is chosen; `Enter` or a click chooses it, and the whole
  app redraws in it at once. `Esc` puts the chosen one back. The option names are in the interface's face: drawing each
  in its own would load every font on the machine to open a list.

  Code's list starts with the two faces that come with knobas (2026-11-21): Cascadia Mono, and Cascadia Code, the one
  with ligatures, which are the *Ligatures in code* switch's two places: picking one clears the chosen family and sets
  the switch. A font installed under one of their names reads "Cascadia Code (installed)", its version after it, and
  draws the machine's copy; typed back with that suffix it is that copy again. The machine's list comes from the
  backend's read of the font folders, which on Windows misses the Cascadias that Windows Terminal brings (WebView2
  draws them by name), so the webview is asked whether those two names draw.

  The size is a slider of the ten steps, 85 to 130 %, the arrow keys stepping it. A step is marked where the chosen
  face's lower case is as tall as the bundled face's (its x-height, which the backend reads); nothing sets itself.
  The specimen is drawn as the app draws the role, through the role's own custom properties, so what it shows is what
  the app now shows.
-->
<script lang="ts" module>
  import { installedFonts as readInstalledFonts, type InstalledFont } from "../../ipc/app";

  /** The families, read once a session: Settings opened again draws them at once. */
  let asked: Promise<InstalledFont[]> | null = null;
  export function installedOnce(read: () => Promise<InstalledFont[]>): Promise<InstalledFont[]> {
    asked ??= read().catch((cause: unknown) => {
      asked = null;
      throw cause;
    });
    return asked;
  }

  /** Whether the webview draws `family` (an installed font the backend's read may miss): it measures differently over a serif and a monospace fallback. */
  export function webviewDraws(family: string): boolean {
    try {
      const ctx = document.createElement("canvas").getContext("2d");
      if (!ctx) return false;
      const width = (stack: string) => {
        ctx.font = `72px ${stack}`;
        return ctx.measureText("mmmmmmmmmmlli0O").width;
      };
      const name = `"${family.replace(/"/g, '\\"')}"`;
      return width(`${name}, serif`) !== width("serif") && width(`${name}, serif`) === width(`${name}, monospace`);
    } catch {
      return false;
    }
  }

  /** Tests only: forget the session's read. */
  export function forgetInstalledFonts(): void {
    asked = null;
  }
</script>

<script lang="ts">
  import { flushSync, untrack } from "svelte";

  import { Windowed } from "../../shell/windowed.svelte";
  import {
    BUNDLED_CODE,
    CASCADIA_CODE,
    CASCADIA_MONO,
    CODE_LINES,
    DEFAULT_LINES,
    cssFamily,
    FAMILY_PROPERTY,
    evenStep,
    familyName,
    fonts as windowFonts,
    KNOB_HOME,
    KNOBS,
    knobSteps,
    ROLE_NAMES,
    shownName,
    SIZES,
    type Fonts,
    type Knob,
    type Role,
  } from "../fonts.svelte";
  import TfNotches from "./TfNotches.svelte";

  let {
    role,
    installedFonts = readInstalledFonts,
    fonts = windowFonts,
    draws = webviewDraws,
  }: {
    role: Role;
    /** The installed families; the IPC read when omitted. */
    installedFonts?: () => Promise<InstalledFont[]>;
    /** The settings; the window's when omitted. */
    fonts?: Fonts;
    /** Whether the webview draws a family the read did not list; a canvas measure when omitted. */
    draws?: (family: string) => boolean;
  } = $props();

  const uid = $props.id();
  const listId = `tf-fr-${uid}-list`;
  const name = $derived(ROLE_NAMES[role]);
  /** The face the role draws in with none chosen: for code, the release's Cascadia Code, or its Cascadia Mono with ligatures off (#2516). */
  const bundled = $derived(fonts.bundled(role));

  /**
   * Code's specimen (2026-11-21, Björn: "a better font preview with ascii boxes, ascii file trees and ligatures"): a
   * box whose stems drop into the next row, a tree whose `│` runs down three rows (both joined at the face's own line height), and a
   * line of the pairs ligatures join, `->` `=>` `>=` `!=` `===`, then `0O 1lI`, which tell faces apart.
   */
  const SPECIMEN: [string, string | null][][] = [
    [["// how a payout is sent again", "tf-hl-com"]],
    [["┌───────────┐  transient  ┌───────────┐", null]],
    [["│  pending  │ ──────────> │  waiting  │", null]],
    [["└─────┬─────┘             └─────┬─────┘", null]],
    [["      │ ok                      │ 24 h", null]],
    [["src/", null]],
    [["├── sepa/", null]],
    [["│   └── retry.rs   ", null], ["-> backoff, 3 tries", "tf-hl-com"]],
    [["└── psp.rs         ", null], ["=> the PSP's answers", "tf-hl-com"]],
    [["if", "tf-hl-kw"], [" (tries >= ", null], ["3", "tf-hl-num"], [" && code != ", null], ["503", "tf-hl-num"], [" || state === ", null], ['"gave up"', "tf-hl-str"], [") ", null], ["return", "tf-hl-kw"], [";  ", null], ["// 0O 1lI", "tf-hl-com"]],
  ];

  /** What each role says it draws, under its name. */
  const WHAT: Record<Role, string> = {
    ui: "Menus, lists, fields and buttons, and the text of READMEs, pull requests and tickets.",
    head: "Page and section titles, every heading in Markdown, the job key and the big figures. Drawn narrow where the font can be.",
    read: "Docs pages.",
    code: "Code blocks, files' source, diffs, build logs and keys everywhere.",
  };

  /** The role's own kind of font, listed first, and what its two headings say. */
  const KIND: Record<Role, { first: (font: InstalledFont) => boolean; heads: [string, string] }> = {
    ui: { first: (font) => !font.monospace && !font.serif, heads: ["Sans serif", "Other fonts"] },
    head: { first: (font) => font.narrows, heads: ["Draws narrow", "Normal width only"] },
    read: { first: (font) => font.serif, heads: ["Serif", "Other fonts"] },
    code: { first: (font) => font.monospace, heads: ["Monospace", "Other fonts"] },
  };

  /** `null` until the read answers, and for good if it fails: the field still takes a typed name. */
  let installed = $state<InstalledFont[] | null>(null);
  $effect(() => {
    void installedOnce(installedFonts)
      .then((found) => (installed = role === "code" ? withWebviewCascadias(found) : found))
      .catch(() => (installed = null));
  });

  /** The read's families, and an installed Cascadia Mono or Code the webview draws that the read missed (no version known). */
  function withWebviewCascadias(found: InstalledFont[]): InstalledFont[] {
    const names = new Set(found.map((font) => font.family.toLowerCase()));
    const more = [CASCADIA_MONO, CASCADIA_CODE]
      .filter((family) => !names.has(family.toLowerCase()) && draws(family))
      .map((family): InstalledFont => ({ family, monospace: true, serif: false, narrows: false, x_height: null, version: null }));
    return [...found, ...more];
  }

  /** What the field holds: the chosen family, or nothing for the bundled face. */
  let text = $state(untrack(() => shownOf(fonts.family(role))));
  let open = $state(false);
  /** Whether the person typed since the list opened: until then it lists every family. */
  let typed = $state(false);
  /** The option the keys or the pointer are on, as an index into `rows`; -1 for none. */
  let active = $state(-1);

  /**
   * A heading, or a face: `pick` is what choosing it hands `setFamily` (a bundled face's plain name, or an installed
   * one's shown name), `css` the family the specimen draws it in, `note` what follows its name, dimmed.
   */
  type Row =
    | { kind: "head"; label: string }
    | { kind: "font"; key: string; label: string; note: string | null; pick: string; css: string; font: InstalledFont | null };

  /** Code's faces that come with knobas: each release's Mono and Code, the switch's two places within it (2026-11-21). */
  const OWN = BUNDLED_CODE.map((face) => ({ label: face.name, note: face.note, css: face.css }));

  const rows = $derived.by((): Row[] => {
    const query = typed ? text.trim().toLowerCase() : "";
    const matches = (label: string) => query === "" || label.toLowerCase().includes(query);
    const shown = (installed ?? []).filter((font) => matches(shownName(font.family)));
    const { first, heads } = KIND[role];
    // Each group by name: the backend lists monospace faces first (the code font's order), which put Courier New and
    // the MingLiU faces at the head of Reading's serifs and Lucida Console at the head of Headings' narrow faces.
    const byName = (a: InstalledFont, b: InstalledFont) => a.family.localeCompare(b.family, "en", { sensitivity: "base" });
    const mine = shown.filter(first).sort(byName);
    const other = shown.filter((font) => !first(font)).sort(byName);
    const row = (font: InstalledFont): Row => {
      const label = shownName(font.family);
      return { kind: "font", key: `font:${font.family}`, label, note: label === font.family ? null : (font.version ?? null), pick: label, css: font.family, font };
    };
    const own = role === "code" ? OWN.filter((face) => matches(face.label)) : [];
    return [
      ...(own.length > 0 ? [{ kind: "head", label: "Comes with knobas" } as Row] : []),
      ...own.map((face): Row => ({ kind: "font", key: `own:${face.label}`, label: face.label, note: face.note, pick: face.label, css: face.css, font: null })),
      ...(mine.length > 0 ? [{ kind: "head", label: heads[0] } as Row] : []),
      ...mine.map(row),
      ...(other.length > 0 ? [{ kind: "head", label: heads[1] } as Row] : []),
      ...other.map(row),
    ];
  });

  const win = new Windowed({ count: () => rows.length, key: (i) => keyOf(rows[i]!), estimate: 28, scrolls: true });

  function keyOf(row: Row): string {
    return row.kind === "head" ? `head:${row.label}` : row.key;
  }

  /** Whether a row is the face code draws now: a bundled one with nothing chosen and the switch at it, else the chosen family. */
  function selected(row: Row & { kind: "font" }): boolean {
    const chosen = fonts.family(role);
    if (row.font === null) return chosen === null && row.label === bundled;
    return chosen !== null && row.font.family.toLowerCase() === chosen.toLowerCase();
  }

  /**
   * What the specimen draws: the option under the keys or the pointer while the list is open, else the chosen face.
   * `family` is the installed family (`null` for a bundled face), `css` the name it is drawn by, `name` its caption.
   */
  const previewed = $derived.by((): { family: string | null; css: string; name: string } => {
    const row = open ? rows[active] : undefined;
    if (row?.kind === "font") {
      const family = row.font === null ? null : familyName(row.pick, role);
      return { family, css: family ?? row.css, name: family ?? row.label };
    }
    const family = fonts.family(role);
    return { family, css: family ?? fonts.bundledCss(role), name: family ?? bundled };
  });

  /** `family`'s row among the installed ones; `null` while that is not known, `undefined` when it is not there. */
  function found(family: string): InstalledFont | null | undefined {
    if (installed === null) return null;
    const lower = family.toLowerCase();
    return installed.find((font) => font.family.toLowerCase() === lower);
  }

  const missing = $derived(previewed.family !== null && found(previewed.family) === undefined);
  /** A chosen Headings face that has no width axis and no condensed members draws at its normal width. */
  const wide = $derived(role === "head" && previewed.family !== null && found(previewed.family)?.narrows === false);

  const size = $derived(fonts.size(role));
  /** The step where the chosen face's lower case is as tall as the bundled face's, if it is on the scale. */
  const even = $derived.by((): number | null => {
    const family = fonts.family(role);
    if (family === null) return null;
    return evenStep(role, found(family)?.x_height);
  });

  /** A chosen family as the field holds it; nothing for the bundled face. */
  function shownOf(family: string | null): string {
    return family === null ? "" : shownName(family);
  }

  function choose(typedName: string | null): void {
    fonts.setFamily(role, typedName);
    text = shownOf(fonts.family(role));
    open = false;
    typed = false;
    active = -1;
  }

  /** Back to what is chosen, the list closed. */
  function restore(): void {
    text = shownOf(fonts.family(role));
    open = false;
    typed = false;
    active = -1;
  }

  function step(by: 1 | -1): void {
    if (!open) open = true;
    let at = active;
    for (let left = rows.length; left > 0; left--) {
      at = at + by;
      if (at < 0 || at >= rows.length) return;
      if (rows[at]!.kind === "font") break;
    }
    if (rows[at]?.kind !== "font") return;
    active = at;
    win.reveal(at);
    flushSync();
    // jsdom draws no boxes, so it has no `scrollIntoView`.
    document.getElementById(`${listId}-${at}`)?.scrollIntoView?.({ block: "nearest" });
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      step(event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const row = open ? rows[active] : undefined;
      choose(row?.kind === "font" ? row.pick : text);
    } else if (event.key === "Escape" && (open || text !== shownOf(fonts.family(role)))) {
      // Handled here: the dialog under Settings does not also close.
      event.preventDefault();
      event.stopPropagation();
      restore();
    }
  }

  /** A name typed and left is chosen, as `Enter` would; a pick from the list was already. */
  function onLeave(): void {
    if (text.trim() !== shownOf(fonts.family(role))) choose(text);
    else restore();
  }

  /** Each knob as its line names it (Fable's ruling 6: Size, Line spacing, Letter spacing, Text weight). */
  const KNOB_NAMES: Record<Knob, string> = { lh: "Line spacing", ls: "Letter spacing", wt: "Text weight" };
  /** The order of a role's knob lines under its size: line spacing first, weight last. */
  const KNOB_ORDER: Knob[] = ["lh", "ls", "wt"];
  const knobs = $derived(KNOB_ORDER.filter((knob) => KNOBS[role].includes(knob)));

  /** A knob's value as its figure says it: letter spacing in em, line spacing a ratio, a weight its number. */
  function knobText(knob: Knob, at: number): string {
    if (knob === "ls") return at === 0 ? "0 em" : `${at > 0 ? "+" : "−"}${(Math.abs(at) / 100).toFixed(2)} em`;
    if (knob === "lh") return (at / 100).toFixed(2);
    return String(at);
  }

  /** What *Back to* says for each knob. */
  function homeText(knob: Knob): string {
    return knob === "ls" ? "0" : knobText(knob, KNOB_HOME[knob]);
  }

  /** The face code draws now, as the notes name it. */
  const drawnFace = $derived(fonts.family(role) === null ? bundled : shownOf(fonts.family(role)));
  /** Code's line spacing: the notch chosen, or none for *Joined* and *Open*. */
  const lineNotch = $derived(role === "code" && typeof fonts.lines === "number" ? fonts.lines : null);
  /** The ratio drawn now, which the arrows step from with a preset pressed. */
  const lineFrom = $derived(role !== "code" ? null : fonts.lines === "joined" ? Math.round(fonts.faceLine * 100) : fonts.lines === "open" ? 160 : fonts.lines);
  const joinsUpTo = $derived(role === "code" ? fonts.joinsUpTo : null);
  /** Code's line spacing figure (Fable's ruling 3). */
  const linesFigure = $derived(
    role !== "code" ? "" : fonts.lines === "joined" ? fonts.faceLine.toFixed(2) : fonts.lines === "open" ? "1.5 to 1.6" : (fonts.lines / 100).toFixed(2),
  );
  /** Code's line spacing note, one line, always there (Fable's ruling 3). */
  const linesNote = $derived.by((): string => {
    if (role !== "code") return "";
    const lines = fonts.lines;
    if (lines === "joined") {
      const joins = joinsUpTo !== null && joinsUpTo / 100 >= fonts.faceLine - 0.025;
      return `Rows are as tall as ${drawnFace} itself, ${fonts.faceLine.toFixed(2)} times the size.${joins ? " Box lines and trees join." : ""}`;
    }
    if (lines === "open") {
      const gaps = joinsUpTo !== null && joinsUpTo < 160;
      return `The spacing before: 1.6 in files and diffs, 1.55 in logs, 1.5 in short logs.${gaps ? ` ${drawnFace}'s box lines leave gaps.` : ""}`;
    }
    const upTo = joinsUpTo === null ? "" : ` ${drawnFace}'s box lines join up to the ring at ${(joinsUpTo / 100).toFixed(2)}.`;
    return `${(lines / 100).toFixed(2)} times the size.${upTo}`;
  });

  /**
   * The specimen's face: the role's family list on the specimen alone, the face under the keys or the pointer first,
   * then the window's list, so its rules draw it exactly as they draw the app and a missing name falls back as it
   * does there. Through the CSSOM: CSP refuses inline styles in markup.
   */
  function drawnIn(face: { family: string | null; css: string }) {
    return (node: HTMLElement) => {
      const property = FAMILY_PROPERTY[role];
      const stack = getComputedStyle(document.documentElement).getPropertyValue(property).trim();
      const first = cssFamily(face.css);
      node.style.setProperty(property, stack === "" ? first : `${first}, ${stack}`);
    };
  }
</script>

<section class="tf-fr tf-fr-{role}" aria-labelledby="tf-fr-{uid}-name">
  <div class="tf-stg-row tf-fr-row">
    <label class="tf-stg-l" for="tf-fr-{uid}"
      ><span id="tf-fr-{uid}-name">{name}</span><small>{WHAT[role]}</small></label
    >
    <div class="tf-stg-c tf-fr-c">
      <div class="tf-fr-line">
        <div class="tf-combo tf-cf-combo tf-fr-combo">
          <input
            id="tf-fr-{uid}"
            type="text"
            class="tf-stg-in"
            role="combobox"
            autocomplete="off"
            spellcheck="false"
            placeholder="{bundled} (comes with knobas)"
            aria-label="{name} font"
            aria-expanded={open}
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={open && active >= 0 ? `${listId}-${active}` : undefined}
            bind:value={text}
            onclick={() => (open = true)}
            oninput={() => ((open = true), (typed = true), (active = -1))}
            onblur={onLeave}
            onkeydown={onKey}
          />
          {#if open && rows.length > 0}
            <div class="tf-opts tf-cf-opts" id={listId} role="listbox" aria-label="Installed fonts" {@attach win.list}>
              {#if win.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("before")}></div>{/if}
              {#each win.slice(rows) as row, i (keyOf(row))}
                {@const at = win.start + i}
                {#if row.kind === "head"}
                  <div class="tf-cf-mh" role="presentation" {@attach win.row(keyOf(row))}>{row.label}</div>
                {:else}
                  <div
                    id="{listId}-{at}"
                    role="option"
                    tabindex="-1"
                    aria-selected={selected(row)}
                    class:tf-act={at === active}
                    {@attach win.row(keyOf(row))}
                    onpointermove={() => (active = at)}
                    onmousedown={(event) => {
                      event.preventDefault();
                      choose(row.pick);
                    }}
                  >
                    {row.label}{#if row.note}{" "}<span class="tf-fr-note">{row.note}</span>{/if}
                  </div>
                {/if}
              {/each}
              {#if win.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("after")}></div>{/if}
            </div>
          {/if}
        </div>
        {#if fonts.family(role) !== null}
          <button type="button" class="tf-btn tf-sm" onclick={() => choose(null)}>Use {bundled}</button>
        {/if}
      </div>
      <div class="tf-fr-line tf-fr-knob tf-fr-sizeline">
        <span class="tf-fr-k" aria-hidden="true">Size</span>
        <TfNotches
          label="{name} size"
          notches={SIZES}
          value={size}
          home={100}
          ring={even}
          ringTitle="lower case as tall as {bundled}'s"
          text={(at) => `${at} %`}
          onPick={(at) => fonts.setSize(role, at)}
        />
        <output class="tf-fr-fig" for="tf-fr-{uid}">{size} %</output>
        {#if size !== 100}
          <button type="button" class="tf-btn tf-sm" onclick={() => fonts.setSize(role, 100)}>Back to 100 %</button>
        {/if}
      </div>
      {#if even !== null && even !== size}
        <p class="tf-fr-hint">The ring at {even} % draws {shownOf(fonts.family(role))}'s lower case as tall as {bundled}'s.</p>
      {/if}
      {#if role === "code"}
        <div class="tf-fr-line tf-fr-knob tf-fr-lines">
          <span class="tf-fr-k" aria-hidden="true">Line spacing</span>
          <div class="tf-seg tf-fr-seg" role="group" aria-label="Code line spacing">
            <button type="button" aria-pressed={fonts.lines === "joined"} onclick={() => fonts.setLines("joined")}>Joined</button>
            <button type="button" aria-pressed={fonts.lines === "open"} onclick={() => fonts.setLines("open")}>Open</button>
          </div>
          <TfNotches
            label="Code line spacing"
            notches={CODE_LINES}
            value={lineNotch}
            home={DEFAULT_LINES}
            from={lineFrom}
            ring={joinsUpTo}
            ringTitle="the tallest at which box lines join"
            text={(at) => (at / 100).toFixed(2)}
            onPick={(at) => fonts.setLines(at)}
            onHome={() => fonts.setLines(DEFAULT_LINES)}
          />
          <output class="tf-fr-fig">{linesFigure}</output>
          {#if fonts.lines !== DEFAULT_LINES}
            <button type="button" class="tf-btn tf-sm" onclick={() => fonts.setLines(DEFAULT_LINES)}>Back to {(DEFAULT_LINES / 100).toFixed(2)}</button>
          {/if}
        </div>
        <p class="tf-fr-hint tf-fr-linesnote">{linesNote}</p>
      {/if}
      {#each knobs as knob (knob)}
        {@const at = fonts.knob(role, knob)}
        <div class="tf-fr-line tf-fr-knob tf-fr-k-{knob}">
          <span class="tf-fr-k" aria-hidden="true">{KNOB_NAMES[knob]}</span>
          <TfNotches
            label="{name} {KNOB_NAMES[knob].toLowerCase()}"
            notches={knobSteps(role, knob)}
            value={at}
            home={KNOB_HOME[knob]}
            text={(n) => knobText(knob, n)}
            onPick={(n) => fonts.setKnob(role, knob, n)}
          />
          <output class="tf-fr-fig">{knobText(knob, at)}</output>
          {#if at !== KNOB_HOME[knob]}
            <button type="button" class="tf-btn tf-sm" onclick={() => fonts.setKnob(role, knob, KNOB_HOME[knob])}>Back to {homeText(knob)}</button>
          {/if}
        </div>
        {#if role === "code" && knob === "ls"}
          <p class="tf-fr-hint">Ligatures draw only at 0. Above 0, box lines show gaps.</p>
        {/if}
      {/each}
    </div>
    <figure class="tf-cf-spec tf-fr-spec">
      {#if role === "code"}
        <pre class="tf-code" aria-hidden="true" {@attach drawnIn(previewed)}>{#each SPECIMEN as line, i (i)}<div class="tf-l"><span>{i + 1}</span><span>{#each line as [part, kind], j (j)}{#if kind}<span class={kind}>{part}</span>{:else}{part}{/if}{/each}</span></div>{/each}</pre>
      {:else if role === "ui"}
        <div class="tf-fr-sample tf-fr-sample-ui" aria-hidden="true" {@attach drawnIn(previewed)}>
          <b>Release checklist for 4.2</b>
          <span>Waiting on two reviews, updated 3 hours ago</span>
          <span class="tf-fr-chips"><span class="tf-chip">In review</span><span class="tf-btn tf-sm">Open in Code</span></span>
        </div>
      {:else if role === "head"}
        <div class="tf-fr-sample tf-fr-sample-head" aria-hidden="true" {@attach drawnIn(previewed)}>
          <span class="tf-fr-key">KNB-1482</span>
          <span class="tf-fr-h">Pull requests waiting on you</span>
        </div>
      {:else}
        <div class="tf-fr-sample tf-fr-sample-read" aria-hidden="true" {@attach drawnIn(previewed)}>
          <p>
            The release train leaves on Thursdays. Anything merged after noon on Wednesday waits for the next one, so ask
            for reviews early in the week. A fix that cannot wait goes out as a patch release from the branch the train
            left from, after two people have read it and the payout tests have passed against the sandbox. Write down in
            the ticket why it could not wait.
          </p>
        </div>
      {/if}
      <figcaption role="status">
        {#if missing}
          {previewed.name} is not installed on this machine, so this draws as {bundled}.
        {:else if wide}
          {previewed.name} has no narrow width, so headings and the job key draw at its normal width.
        {:else}
          {previewed.family === null ? previewed.name : shownName(previewed.name)}, {size} %
        {/if}
      </figcaption>
    </figure>
  </div>
  {#if role === "code"}
    <div class="tf-stg-row">
      <label class="tf-stg-l" for="tf-fr-{uid}-lig"
        >Ligatures in code<small
          >Draws <code>=&gt;</code>, <code>!=</code>, <code>-&gt;</code> and the like as one sign, in Cascadia Code instead
          of Cascadia Mono, or in a font you chose that has them. Each still takes the width of its characters; ASCII art
          and boxes draw best with them off.</small
        ></label
      >
      <div class="tf-stg-c tf-stg-inl">
        <button
          id="tf-fr-{uid}-lig"
          type="button"
          class="tf-stg-sw"
          role="switch"
          aria-checked={fonts.ligatures}
          aria-label="Ligatures in code"
          onclick={() => fonts.setLigatures(!fonts.ligatures)}><i></i></button
        >
      </div>
    </div>
  {/if}
</section>
