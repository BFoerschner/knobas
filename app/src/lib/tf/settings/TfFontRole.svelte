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

  /** Tests only: forget the session's read. */
  export function forgetInstalledFonts(): void {
    asked = null;
  }
</script>

<script lang="ts">
  import { flushSync, untrack } from "svelte";

  import { Windowed } from "../../shell/windowed.svelte";
  import { cssFamily, FAMILY_PROPERTY, evenStep, familyName, fonts as windowFonts, LARGEST, ROLE_NAMES, SIZES, SMALLEST, type Fonts, type Role } from "../fonts.svelte";

  let {
    role,
    installedFonts = readInstalledFonts,
    fonts = windowFonts,
  }: {
    role: Role;
    /** The installed families; the IPC read when omitted. */
    installedFonts?: () => Promise<InstalledFont[]>;
    /** The settings; the window's when omitted. */
    fonts?: Fonts;
  } = $props();

  const uid = $props.id();
  const listId = `tf-fr-${uid}-list`;
  const name = $derived(ROLE_NAMES[role]);
  /** The face the role draws in with none chosen: for code, Cascadia Mono, or Cascadia Code with ligatures on (2026-11-21). */
  const bundled = $derived(fonts.bundled(role));

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
      .then((found) => (installed = found))
      .catch(() => (installed = null));
  });

  /** What the field holds: the chosen family, or nothing for the bundled face. */
  let text = $state(untrack(() => fonts.family(role) ?? ""));
  let open = $state(false);
  /** Whether the person typed since the list opened: until then it lists every family. */
  let typed = $state(false);
  /** The option the keys or the pointer are on, as an index into `rows`; -1 for none. */
  let active = $state(-1);

  type Row = { kind: "head"; label: string } | { kind: "font"; font: InstalledFont };

  const rows = $derived.by((): Row[] => {
    const query = typed ? text.trim().toLowerCase() : "";
    const shown = (installed ?? []).filter((font) => query === "" || font.family.toLowerCase().includes(query));
    const { first, heads } = KIND[role];
    // Each group by name: the backend lists monospace faces first (the code font's order), which put Courier New and
    // the MingLiU faces at the head of Reading's serifs and Lucida Console at the head of Headings' narrow faces.
    const byName = (a: InstalledFont, b: InstalledFont) => a.family.localeCompare(b.family, "en", { sensitivity: "base" });
    const mine = shown.filter(first).sort(byName);
    const other = shown.filter((font) => !first(font)).sort(byName);
    return [
      ...(mine.length > 0 ? [{ kind: "head", label: heads[0] } as Row] : []),
      ...mine.map((font): Row => ({ kind: "font", font })),
      ...(other.length > 0 ? [{ kind: "head", label: heads[1] } as Row] : []),
      ...other.map((font): Row => ({ kind: "font", font })),
    ];
  });

  const win = new Windowed({ count: () => rows.length, key: (i) => keyOf(rows[i]!), estimate: 28, scrolls: true });

  function keyOf(row: Row): string {
    return row.kind === "head" ? `head:${row.label}` : `font:${row.font.family}`;
  }

  /** The family the specimen draws: the option under the keys or the pointer while the list is open, else the chosen one. */
  const previewed = $derived.by((): string | null => {
    const row = open ? rows[active] : undefined;
    if (row?.kind === "font") return familyName(row.font.family, role);
    return fonts.family(role);
  });

  /** `family`'s row among the installed ones; `null` while that is not known, `undefined` when it is not there. */
  function found(family: string): InstalledFont | null | undefined {
    if (installed === null) return null;
    const lower = family.toLowerCase();
    return installed.find((font) => font.family.toLowerCase() === lower);
  }

  const missing = $derived(previewed !== null && found(previewed) === undefined);
  /** A chosen Headings face that has no width axis and no condensed members draws at its normal width. */
  const wide = $derived(role === "head" && previewed !== null && found(previewed)?.narrows === false);

  const size = $derived(fonts.size(role));
  /** The step where the chosen face's lower case is as tall as the bundled face's, if it is on the scale. */
  const even = $derived.by((): number | null => {
    const family = fonts.family(role);
    if (family === null) return null;
    return evenStep(role, found(family)?.x_height);
  });

  function choose(typedName: string | null): void {
    fonts.setFamily(role, typedName);
    text = fonts.family(role) ?? "";
    open = false;
    typed = false;
    active = -1;
  }

  /** Back to what is chosen, the list closed. */
  function restore(): void {
    text = fonts.family(role) ?? "";
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
      choose(row?.kind === "font" ? row.font.family : text);
    } else if (event.key === "Escape" && (open || text !== (fonts.family(role) ?? ""))) {
      // Handled here: the dialog under Settings does not also close.
      event.preventDefault();
      event.stopPropagation();
      restore();
    }
  }

  /** A name typed and left is chosen, as `Enter` would; a pick from the list was already. */
  function onLeave(): void {
    if (text.trim() !== (fonts.family(role) ?? "")) choose(text);
    else restore();
  }

  /** The slider's keys: one step with the arrows, the ends with Home and End, 100 % with 0. */
  function onSizeKey(event: KeyboardEvent): void {
    const at = SIZES.indexOf(size as (typeof SIZES)[number]);
    let to: number | null = null;
    if (event.key === "ArrowRight" || event.key === "ArrowUp") to = SIZES[Math.min(SIZES.length - 1, at + 1)]!;
    else if (event.key === "ArrowLeft" || event.key === "ArrowDown") to = SIZES[Math.max(0, at - 1)]!;
    else if (event.key === "Home") to = SMALLEST;
    else if (event.key === "End") to = LARGEST;
    else if (event.key === "0") to = 100;
    if (to === null) return;
    event.preventDefault();
    fonts.setSize(role, to);
  }

  /**
   * The specimen's face: the role's family list on the specimen alone, the family under the keys or the pointer first,
   * then the window's list, so its rules draw it exactly as they draw the app and a missing name falls back as it
   * does there. Through the CSSOM: CSP refuses inline styles in markup.
   */
  function drawnIn(family: string | null) {
    return (node: HTMLElement) => {
      const property = FAMILY_PROPERTY[role];
      const stack = getComputedStyle(document.documentElement).getPropertyValue(property).trim();
      const first = cssFamily(family ?? fonts.bundled(role));
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
                    aria-selected={row.font.family.toLowerCase() === (fonts.family(role) ?? bundled).toLowerCase()}
                    class:tf-act={at === active}
                    {@attach win.row(keyOf(row))}
                    onpointermove={() => (active = at)}
                    onmousedown={(event) => {
                      event.preventDefault();
                      choose(row.font.family);
                    }}
                  >
                    {row.font.family}
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
      <div class="tf-fr-line tf-fr-sizeline">
        <div
          class="tf-fr-scale"
          role="slider"
          tabindex="0"
          aria-label="{name} size"
          aria-valuemin={SMALLEST}
          aria-valuemax={LARGEST}
          aria-valuenow={size}
          aria-valuetext="{size} %"
          onkeydown={onSizeKey}
        >
          {#each SIZES as at (at)}
            <button
              type="button"
              tabindex="-1"
              class="tf-fr-notch"
              class:tf-fr-home={at === 100}
              class:tf-fr-even={at === even}
              class:tf-fr-on={at === size}
              aria-hidden="true"
              title={at === even ? `${at} %: lower case as tall as ${bundled}'s` : `${at} %`}
              onclick={() => fonts.setSize(role, at)}><i></i></button
            >
          {/each}
        </div>
        <output class="tf-fr-fig" for="tf-fr-{uid}">{size} %</output>
        {#if size !== 100}
          <button type="button" class="tf-btn tf-sm" onclick={() => fonts.setSize(role, 100)}>Back to 100 %</button>
        {/if}
      </div>
      {#if even !== null && even !== size}
        <p class="tf-fr-hint">The ring at {even} % draws {fonts.family(role)}'s lower case as tall as {bundled}'s.</p>
      {/if}
    </div>
    <figure class="tf-cf-spec tf-fr-spec">
      {#if role === "code"}
        <pre class="tf-code" aria-hidden="true" {@attach drawnIn(previewed)}><div class="tf-l"><span>1</span><span><span class="tf-hl-kw">const</span> failed = runs.<span class="tf-hl-fn">filter</span>((run) =&gt; run.state !== <span class="tf-hl-str">"ok"</span>);</span></div><div class="tf-l"><span>2</span><span><span class="tf-hl-kw">if</span> (failed.length &gt;= <span class="tf-hl-num">3</span>) <span class="tf-hl-kw">return</span> <span class="tf-hl-fn">retry</span>(failed, &#123; after: <span class="tf-hl-num">0x1F</span> &#125;);</span></div><div class="tf-l"><span>3</span><span><span class="tf-hl-com">// 0O 1lI -&gt; =&gt; != === |-- ┌─┐</span></span></div></pre>
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
            for reviews early in the week.
          </p>
        </div>
      {/if}
      <figcaption role="status">
        {#if missing}
          {previewed} is not installed on this machine, so this draws as {bundled}.
        {:else if wide}
          {previewed} has no narrow width, so headings and the job key draw at its normal width.
        {:else}
          {previewed ?? bundled}, {size} %
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
