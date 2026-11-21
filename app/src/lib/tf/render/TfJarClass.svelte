<!--
  A class inside a JAR (ux19), outlined as `javap` would, read in Rust without Java: its annotations and declaration,
  what it extends, implements and permits, then its fields, constructors and methods, one a line in the code face,
  annotations above in `--ink-2`. A record shows its components in its declaration, an enum its constants first, a
  module its requires, exports and opens.

  *Outline* / *Source* at the head's end where a Java and the bundled decompiler can run (Fable's ruling): *Source*
  decompiles the class with Vineflower, an inner class as its top-level one, in a pane whose height is held while it
  works. With a Java too old, *Decompile* is there disabled, the reason beside it. With no Java, neither is drawn.
-->
<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";

  import type { ClassOutline, Member } from "../../ipc/entity";
  import type { JavaRuntime } from "../../ipc/app";
  import { declaration, memberLine, supers, topClass } from "../../render/jar";
  import { renderPort, type RenderPort } from "../../render/port";
  import TfJarCode from "./TfJarCode.svelte";

  let {
    outline,
    path,
    java,
    source,
    mode,
    onmode,
    render = renderPort(),
  }: {
    outline: ClassOutline;
    /** The entry's path in the archive. */
    path: string;
    /** What decompiling stands at; `null` while it is looked for. */
    java: JavaRuntime | null;
    /** The decompiled source: `null` before it is asked, `"reading"` while it is made. */
    source: { text: string } | { error: string } | "reading" | null;
    mode: "outline" | "source";
    onmode: (mode: "outline" | "source") => void;
    render?: RenderPort;
  } = $props();

  /** A group's first members drawn; a generated class's thousands wait for *Show the other …*, below them. */
  const MEMBERS = 200;
  const all = new SvelteSet<string>();

  const constants = $derived(outline.fields.filter((f) => f.constant));
  const fields = $derived(outline.fields.filter((f) => !f.constant));
  const simple = $derived(outline.name.slice(outline.name.lastIndexOf(".") + 1).replace(/\$/g, "."));
  const ctors = $derived(outline.methods.filter((m) => m.ty === "" && m.name === simple.slice(simple.lastIndexOf(".") + 1)));
  const methods = $derived(outline.methods.filter((m) => !ctors.includes(m)));
  const top = $derived(topClass(path));
  const topName = $derived(top.slice(top.lastIndexOf("/") + 1));
</script>

{#snippet member(m: Member)}
  <li class="tf-jar-mem" class:tf-jar-dep={m.deprecated}>
    {#each m.annotations as a, n (n)}<span class="tf-jar-ann">{a}</span>{/each}
    <code>{memberLine(m)}</code>{#if m.deprecated}<span class="tf-jar-deplab">Deprecated</span>{/if}
  </li>
{/snippet}

{#snippet group(label: string, list: Member[])}
  {#if list.length > 0}
    <section class="tf-jar-group">
      <h4 class="tf-jar-glabel">{label} <span class="tf-dim">{list.length}</span></h4>
      <ul>
        {#each list.slice(0, MEMBERS) as m, n (n)}{@render member(m)}{/each}
        {#if all.has(label)}{#each list.slice(MEMBERS) as m, n (n)}{@render member(m)}{/each}{/if}
      </ul>
      {#if list.length > MEMBERS && !all.has(label)}
        <p class="tf-jar-more"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => all.add(label)}>Show the other {(list.length - MEMBERS).toLocaleString("en")}</button></p>
      {/if}
    </section>
  {/if}
{/snippet}

<div class="tf-jar-class">
  <div class="tf-jar-chead">
    <span class="tf-jar-cfacts">
      <span>{outline.release}</span>
      {#if outline.source_file !== null}<span class="tf-dim">from <code>{outline.source_file}</code></span>{/if}
    </span>
    <span class="tf-trail">
      {#if java !== null && java.can_decompile}
        <span class="tf-seg" role="group" aria-label="Show the class as">
          <button type="button" aria-pressed={mode === "outline"} onclick={() => onmode("outline")}>Outline</button>
          <button type="button" aria-pressed={mode === "source"} onclick={() => onmode("source")}>Source</button>
        </span>
      {:else if java !== null && java.java !== null && !java.can_decompile && java.reason !== null}
        <span class="tf-dim tf-jar-why">{java.reason}</span>
        <button type="button" class="tf-btn tf-sm" disabled title={java.reason}>Decompile</button>
      {/if}
    </span>
  </div>

  {#if mode === "source"}
    <p class="tf-jar-note">
      {#if top !== path}<code>{topName}</code>, which holds this class, decompiled{:else}Decompiled{/if} by {java?.decompiler ?? "the decompiler"}
      from its bytecode. It is not the original source: names of locals and comments are gone.
    </p>
    {#if source === null || source === "reading"}
      <div class="tf-jar-pending" role="status"><p class="tf-dim">Decompiling {topName}…</p></div>
    {:else if "error" in source}
      <div class="tf-jar-pending" role="alert"><p><b>Could not decompile {topName}.</b></p><pre>{source.error}</pre></div>
    {:else}
      <TfJarCode text={source.text} path={top.replace(/\.class$/, ".java")} max={28} fixed {render} />
    {/if}
  {:else}
    <div class="tf-jar-decl">
      {#each outline.annotations as a, n (n)}<span class="tf-jar-ann">{a}</span>{/each}
      <code class="tf-jar-dline" class:tf-jar-dep={outline.deprecated}>{declaration(outline)}</code>
      {#each supers(outline) as s (s.word)}
        <code class="tf-jar-super"><span class="tf-dim">{s.word}</span> {s.names.join(", ")}</code>
      {/each}
    </div>
    {#if outline.kind === "module"}
      <ul class="tf-jar-group tf-jar-mod">{#each outline.module as line, n (n)}<li class="tf-jar-mem"><code>{line}</code></li>{/each}</ul>
    {/if}
    {#if constants.length > 0}
      <section class="tf-jar-group">
        <h4 class="tf-jar-glabel">Constants <span class="tf-dim">{constants.length}</span></h4>
        <p class="tf-jar-consts"><code>{constants.map((c) => c.name).join(", ")}</code></p>
      </section>
    {/if}
    {@render group("Fields", fields)}
    {@render group("Constructors", ctors)}
    {@render group("Methods", methods)}
    {#if outline.synthetic > 0}
      <p class="tf-jar-note">{outline.synthetic === 1 ? "One member the compiler made is" : `${outline.synthetic} members the compiler made are`} not listed.</p>
    {/if}
  {/if}
</div>
