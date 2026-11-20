<!--
  Settings › Renderers › *Decompiler* (ux19, `docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md`, Fable's
  ruling): what the JAR view's *Decompile* runs with. One line says the Java found and the bundled decompiler, or why
  there is none; *Look again* looks again (a Java installed since); a Java of the person's own (a `java` or a Java
  home, an absolute path) is used before any knobas finds, cleared to look by itself.

  Nothing moves once drawn (`docs/decisions/2026-11-17-nothing-moves-once-drawn.md`; the ux19 gate found the path row
  142 px across and up to 28 px down on the real backend, whose answer lands after the page draws): the cell is a column,
  the line above the path row, and the line and the reason share one room two lines high from the first frame. The line
  takes both lines, or one with the reason on the second; each cut with an ellipsis says its whole in its title. The
  path is a field in a row of controls, as *Default renderers*' pattern is (it drew with no field's box before).
-->
<script lang="ts" module>
  import { javaRuntime, setJavaPath, type JavaRuntime } from "../../ipc/app";

  export interface DecompilerPorts {
    java: (again: boolean) => Promise<JavaRuntime>;
    setPath: (path: string | null) => Promise<JavaRuntime>;
  }

  /** Looked up at the call, so a test that mocks the barrel without these never reaches them. */
  export const realDecompilerPorts: DecompilerPorts = {
    java: (again) => Promise.resolve().then(() => javaRuntime(again)),
    setPath: (path) => Promise.resolve().then(() => setJavaPath(path)),
  };
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import TfPath from "../TfPath.svelte";

  let { ports = realDecompilerPorts }: { ports?: DecompilerPorts } = $props();

  let rt = $state<JavaRuntime | null>(null);
  let looking = $state(true);
  let typed = $state("");
  let refusal = $state<string | null>(null);
  /** Why the first read did not answer. */
  let unread = $state<string | null>(null);

  function took(answer: JavaRuntime) {
    rt = answer;
    typed = answer.set_path ?? "";
    looking = false;
  }

  $effect(() => {
    const read = ports;
    let live = true;
    read.java(false).then(
      (answer) => live && took(answer),
      (error: unknown) => {
        if (!live) return;
        looking = false;
        // Not an action that failed: said in the line, not as an alert.
        unread = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  async function again() {
    looking = true;
    refusal = null;
    try {
      took(await ports.java(true));
    } catch (error) {
      looking = false;
      refusal = ipcErrorMessage(error);
    }
  }

  async function use(path: string | null) {
    looking = true;
    refusal = null;
    try {
      took(await ports.setPath(path));
    } catch (error) {
      looking = false;
      refusal = ipcErrorMessage(error);
    }
  }

  const tool = $derived(rt === null || rt.decompiler === null ? "no decompiler in this build" : `${rt.decompiler} (bundled)`);
  const line = $derived.by(() => {
    if (rt === null) return looking ? "Looking for a Java…" : unread === null ? "" : `Could not tell which Java is here: ${unread}`;
    if (rt.java === null) return `No Java found on this machine, so the JAR view shows outlines only; ${tool}.`;
    return `Java ${rt.java.version} at ${rt.java.path}, with ${tool}.`;
  });
</script>

<div class="tf-stg-row">
  <label class="tf-stg-l" for="tf-rnd-java"
    >Decompiler<small
      >The JAR view's <i>Source</i> on a class runs the decompiler knobas carries with a Java on this machine. knobas downloads
      nothing; with no Java, classes show as outlines.</small
    ></label
  >
  <div class="tf-stg-c tf-dec-c">
    <div class="tf-dec-st">
      <!-- A Java's path is a `TfPath` in a line of its own, so a deep one gives up its middle and keeps java.exe (the renderers-columns pass). -->
      {#if rt !== null && rt.java !== null}
        <p class="tf-dec-line tf-dec-at" role="status" title={line}
          ><span>Java {rt.java.version} at{" "}</span><TfPath path={rt.java.path} title={line} /><span>, with {tool}.</span></p
        >
      {:else}
        <p class="tf-dec-line" class:tf-dec-one={rt !== null && rt.reason !== null} role="status" title={line}>{line}</p>
      {/if}
      {#if rt !== null && rt.reason !== null}<p class="tf-dec-why" title={rt.reason}>{rt.reason}</p>{/if}
    </div>
    <div class="tf-stg-c tf-stg-row1 tf-dec-path">
      <input
        id="tf-rnd-java"
        class="tf-stg-in tf-mono"
        aria-label="The Java to use"
        autocomplete="off"
        spellcheck="false"
        placeholder="Found by itself"
        bind:value={typed}
        onkeydown={(event) => event.key === "Enter" && void use(typed.trim() === "" ? null : typed.trim())}
      />
      <button type="button" class="tf-btn tf-sm" disabled={looking} onclick={() => void use(typed.trim() === "" ? null : typed.trim())}>Use this Java</button>
      <button type="button" class="tf-btn tf-sm tf-ghost" disabled={looking} onclick={() => void again()}>Look again</button>
    </div>
    {#if refusal !== null}<p class="tf-stg-errt" role="alert">{refusal}</p>{/if}
  </div>
</div>

