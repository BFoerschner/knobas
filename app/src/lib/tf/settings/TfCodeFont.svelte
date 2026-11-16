<!--
  Settings › Appearance › *Code font* and *Ligatures in code* (2026-11-16, Björn: "let me set the font generally in
  the settings pane"; "maybe add a toggle in the settings pane for it"). The two settings are `code-face.svelte.ts`'s,
  which writes them on <html> for every place code is drawn.

  The field is a combobox over the families installed on this machine (`installed_fonts`, read once a run by the
  backend), monospace ones first, under a heading each; a name typed that is not listed is taken as it is, since a
  font can be installed where the read does not look. A click, a key typed or `↓` opens the list, focus alone does
  not: Settings puts the focus in a section's first field as it opens. The option the keys or the pointer are on draws in the specimen
  under the field before it is chosen; `Enter` or a click chooses it, and the whole app redraws in it at once. `Esc`
  puts the chosen one back. The specimen is a few lines as the Source view draws a file: numbered, highlighted, at the
  Source view's size, so what it shows is what a file will look like. The option names are in the interface's face:
  drawing each in its own would load every font on the machine to open a list.
-->
<script lang="ts" module>
  import { installedFonts as readInstalledFonts, type InstalledFont } from "../../ipc/app";

  /** The families, read once a session: Settings opened again draws them at once. */
  let asked: Promise<InstalledFont[]> | null = null;
  function installedOnce(read: () => Promise<InstalledFont[]>): Promise<InstalledFont[]> {
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
  import { BUNDLED, codeFace as windowFace, cssFamily, familyName, type CodeFace } from "../code-face.svelte";

  let {
    installedFonts = readInstalledFonts,
    face = windowFace,
  }: {
    /** The installed families; the IPC read when omitted. */
    installedFonts?: () => Promise<InstalledFont[]>;
    /** The setting; the window's when omitted. */
    face?: CodeFace;
  } = $props();

  const uid = $props.id();
  const listId = `tf-cf-${uid}-list`;

  /** `null` until the read answers, and for good if it fails: the field still takes a typed name. */
  let fonts = $state<InstalledFont[] | null>(null);
  $effect(() => {
    void installedOnce(installedFonts)
      .then((found) => (fonts = found))
      .catch(() => (fonts = null));
  });

  /** What the field holds: the chosen family, or nothing for the bundled face. */
  let text = $state(untrack(() => face.family ?? ""));
  let open = $state(false);
  /** Whether the person typed since the list opened: until then it lists every family. */
  let typed = $state(false);
  /** The option the keys or the pointer are on, as an index into `rows`; -1 for none. */
  let active = $state(-1);

  type Row = { kind: "head"; label: string } | { kind: "font"; font: InstalledFont };

  const rows = $derived.by((): Row[] => {
    const query = typed ? text.trim().toLowerCase() : "";
    const shown = (fonts ?? []).filter((font) => query === "" || font.family.toLowerCase().includes(query));
    const mono = shown.filter((font) => font.monospace);
    const other = shown.filter((font) => !font.monospace);
    return [
      ...(mono.length > 0 ? [{ kind: "head", label: "Monospace" } as Row] : []),
      ...mono.map((font): Row => ({ kind: "font", font })),
      ...(other.length > 0 ? [{ kind: "head", label: "Other fonts" } as Row] : []),
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
    if (row?.kind === "font") return familyName(row.font.family);
    return face.family;
  });

  /** Whether `family` is among the installed ones; `null` while that is not known. */
  function installed(family: string): boolean | null {
    if (fonts === null) return null;
    const name = family.toLowerCase();
    return fonts.some((font) => font.family.toLowerCase() === name);
  }

  const missing = $derived(previewed !== null && installed(previewed) === false);

  function choose(name: string | null): void {
    face.setFamily(name);
    text = face.family ?? "";
    open = false;
    typed = false;
    active = -1;
  }

  /** Back to what is chosen, the list closed. */
  function restore(): void {
    text = face.family ?? "";
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
    } else if (event.key === "Escape" && (open || text !== (face.family ?? ""))) {
      // Handled here: the dialog under Settings does not also close.
      event.preventDefault();
      event.stopPropagation();
      restore();
    }
  }

  /** A name typed and left is chosen, as `Enter` would; a pick from the list was already. */
  function onLeave(): void {
    if (text.trim() !== (face.family ?? "")) choose(text);
    else restore();
  }

  /** The specimen's face, through the CSSOM: CSP refuses inline styles in markup. */
  function drawnIn(family: string | null) {
    return (node: HTMLElement) => {
      if (family === null) node.style.removeProperty("font-family");
      else node.style.setProperty("font-family", `${cssFamily(family)}, var(--mono)`);
    };
  }
</script>

<div class="tf-stg-row tf-cf-row">
  <label class="tf-stg-l" for="tf-cf-{uid}"
    >Code font<small
      >Code blocks, files' source, diffs, build logs and keys everywhere. A font this machine does not have draws as
      {BUNDLED}, which comes with knobas.</small
    ></label
  >
  <div class="tf-stg-c tf-stg-inl">
    <div class="tf-combo tf-cf-combo">
      <input
        id="tf-cf-{uid}"
        type="text"
        class="tf-stg-in"
        role="combobox"
        autocomplete="off"
        spellcheck="false"
        placeholder="{BUNDLED} (comes with knobas)"
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
                aria-selected={row.font.family.toLowerCase() === (face.family ?? BUNDLED).toLowerCase()}
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
    {#if face.family !== null}
      <button type="button" class="tf-btn tf-sm" onclick={() => choose(null)}>Use {BUNDLED}</button>
    {/if}
  </div>
  <figure class="tf-cf-spec">
    <pre class="tf-code" aria-hidden="true" {@attach drawnIn(previewed)}><div class="tf-l"><span>1</span><span><span class="tf-hl-kw">const</span> failed = runs.<span class="tf-hl-fn">filter</span>((run) =&gt; run.state !== <span class="tf-hl-str">"ok"</span>);</span></div><div class="tf-l"><span>2</span><span><span class="tf-hl-kw">if</span> (failed.length &gt;= <span class="tf-hl-num">3</span>) <span class="tf-hl-kw">return</span> <span class="tf-hl-fn">retry</span>(failed, &#123; after: <span class="tf-hl-num">0x1F</span> &#125;);</span></div><div class="tf-l"><span>3</span><span><span class="tf-hl-com">// 0O 1lI -&gt; =&gt; &lt;= != === :: |&gt;</span></span></div></pre>
    <figcaption role="status">
      {#if missing}
        {previewed} is not installed on this machine, so code draws as {BUNDLED}.
      {:else}
        {previewed ?? BUNDLED}
      {/if}
    </figcaption>
  </figure>
</div>
<div class="tf-stg-row">
  <label class="tf-stg-l" for="tf-cf-{uid}-lig"
    >Ligatures in code<small
      >Draws <code>=&gt;</code>, <code>!=</code>, <code>-&gt;</code> and the like as one sign, in fonts that have them.
      Each still takes the width of its characters.</small
    ></label
  >
  <div class="tf-stg-c tf-stg-inl">
    <button
      id="tf-cf-{uid}-lig"
      type="button"
      class="tf-stg-sw"
      role="switch"
      aria-checked={face.ligatures}
      aria-label="Ligatures in code"
      onclick={() => face.setLigatures(!face.ligatures)}><i></i></button
    >
  </div>
</div>
