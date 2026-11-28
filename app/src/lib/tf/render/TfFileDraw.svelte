<!--
  **A file, drawn** (#2581): the one dispatch from a file and the renderer chosen for it to the leaf that draws it, for
  the Code page's file view and Settings' *Try it on*, so what is tried is what the Code page draws. `routeOf`
  (`file-draw.ts`) picks the leaf; each route has its branch here.

  - **The file**: its path and text (a pipeline's output, where one ran), and the bytes as read where they stand beside
    it, for the leaves that read bytes (a PDF, a binary image).
  - **The repository** (`repo`, the Code page's): relative links and images, an explorer's place in the address, the
    XML view's schema check, jq and values, the JAR view's archive reads. Without one (*Try it on*) those stay off, and
    a JAR says it is drawn on the Code page.
  - **The page around it**: the frame a page draws in and the room it takes, the command port, and ⌘F's find bar over a
    rendered file's words (`find`, the Code page's; Settings keeps its own ⌘F).

  It adds no element of its own, so the leaf sits where it sat in its caller.
-->
<script lang="ts">
  import type { CommandPort } from "../../render/command";
  import type { DiagramEngine } from "../../render/diagram";
  import { isDelimited } from "../../render/json-sniff";
  import type { RenderPort } from "../../render/port";
  import type { Renderer } from "../../render/registry";
  import TfLines from "../code/TfLines.svelte";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import TfFindBar from "../TfFindBar.svelte";
  import TfBytesImage from "./TfBytesImage.svelte";
  import TfCommandOutput from "./TfCommandOutput.svelte";
  import TfDiagram from "./TfDiagram.svelte";
  import TfHtmlPage from "./TfHtmlPage.svelte";
  import TfPdf from "./TfPdf.svelte";
  import { docViewModule } from "./doc/doc-module";
  import { routeOf, type FileDrawRepo } from "./file-draw";
  import { explorerModule } from "./oas-module";
  import type { SpecOpener } from "./oas-open.svelte";
  import { xsdModule } from "./xsd/xsd-module";

  let {
    drawn,
    renderer,
    bytes = null,
    render,
    commands,
    frame,
    room,
    find = false,
    repo = null,
    typed = $bindable(""),
    opener = $bindable(null),
  }: {
    /** The file as drawn: the pipeline's output where one ran, else the file as read; `null` for none. */
    drawn: { path: string; text: string; binary: boolean } | null;
    /** What draws it: the one picked, else the default. */
    renderer: Renderer | null;
    /** The file's bytes as read and their size, where `drawn` is the file as read and they can be read. */
    bytes?: { read: () => Promise<Uint8Array>; size: number } | null;
    render: RenderPort;
    commands: CommandPort;
    /** Where a page (an HTML file) is drawn, and the element whose room it takes. */
    frame: (document: string) => Promise<string>;
    room: string;
    /** ⌘F's find bar over a rendered file's words. */
    find?: boolean;
    repo?: FileDrawRepo | null;
    /** What is typed into Source's *Find in file*: the Code page keeps it across files. */
    typed?: string;
    /** What *Open in…* opens in an API client while the API explorer draws the file. */
    opener?: SpecOpener | null;
  } = $props();

  const route = $derived(routeOf(renderer, drawn, bytes !== null, repo === null ? "outside" : repo.jar === null ? "unread" : "read"));
  /** A text file drawn as an image (an SVG): its bytes, for `TfBytesImage`, which sanitizes an SVG. */
  const image = $derived.by(() => {
    if (route !== "svg" || drawn === null) return null;
    const bytes = new TextEncoder().encode(drawn.text);
    return { bytes, read: () => Promise.resolve(bytes) };
  });
  /** A rendered file's words, for ⌘F's find bar. */
  let rendered = $state<HTMLElement | null>(null);

  const parentOf = (full: string) => (full.includes("/") ? full.slice(0, full.lastIndexOf("/")) : "");
  const leafOf = (full: string) => full.slice(full.lastIndexOf("/") + 1);
</script>

{#if route === "pdf"}
  <TfPdf read={bytes!.read} size={bytes!.size} {render} />
{:else if route === "image"}
  <TfBytesImage read={bytes!.read} path={drawn!.path} size={bytes!.size} />
{:else if route === "jar"}
  {@const r = repo!}
  <!-- A Java archive (ux19): its view loads with the first archive opened, never at start-up. -->
  {#await import("./TfJar.svelte") then { default: TfJar }}
    <TfJar path={r.file.path} ports={r.jar!} {render} />
  {/await}
{:else if route === "jar-elsewhere"}
  <div class="tf-empty"><p>A JAR is drawn from a repository’s file on the Code page; here it is not drawn.</p></div>
{:else if route === "binary" || drawn === null}
  <div class="tf-empty"><p>A binary file, not drawn.</p></div>
{:else if route === "svg"}
  <TfBytesImage read={image!.read} path={drawn.path} size={image!.bytes.length} />
{:else if route === "openapi"}
  <!-- An OpenAPI spec (ux18): its view and its worker load with the first spec opened, never at start-up. -->
  {@const spec = drawn}
  {@const r = repo}
  {#snippet oasProse(words: string)}
    <TfMarkdown text={words} base={parentOf(spec.path)} fileLink={r?.fileLink ?? null} {render} image={r?.images ?? null} imagesOf={r?.entity ?? ""} />
  {/snippet}
  {#await explorerModule() then { default: TfOpenApi }}
    <TfOpenApi
      text={spec.text}
      path={spec.path}
      read={r?.images ?? null}
      at={r !== null && r.path === spec.path ? r.place : null}
      line={r !== null && r.path === spec.path ? r.line : null}
      repo={r?.id ?? ""}
      bind:opener
      fileHref={r?.placeLink ?? (() => null)}
      oncopylink={r === null ? null : (at, label) => r.copyPlace(spec.path, at, label)}
      onsource={r === null ? null : () => r.source(spec.path)}
      onplace={r === null
        ? null
        : (to, how, label) => {
            const hash = to === "" ? r.hrefOf(spec.path) : r.placeHrefOf(spec.path, to);
            if (how === "push") r.go(hash, label);
            else r.replace(hash);
          }}
      onback={r === null ? null : () => history.back()}
      prose={r === null ? null : oasProse}
    />
  {/await}
{:else if route === "xsd"}
  <!-- An XML Schema (2026-11-23): the schema explorer; its view and its worker load with the first schema opened. -->
  {@const schema = drawn}
  {@const r = repo}
  {#await xsdModule() then { default: TfXsd }}
    <TfXsd
      text={schema.text}
      path={schema.path}
      repo={r?.id ?? ""}
      read={r === null ? () => Promise.resolve(null) : r.text}
      paths={r === null ? () => Promise.resolve([schema.path]) : r.paths}
      readOther={r?.readOther ?? null}
      elsewhere={r?.elsewhere ?? null}
      {render}
      at={r !== null && r.path === schema.path ? r.place : null}
      line={r !== null && r.path === schema.path ? r.line : null}
      oncopylink={r === null ? null : (at, label) => r.copyPlace(schema.path, at, label)}
      onsource={r === null
        ? null
        : (other, n) => {
            if (other.startsWith("@")) return;
            r.source(other);
            r.go(r.lineHrefOf(other, n), leafOf(other));
          }}
      onfile={r === null
        ? null
        : (other) => {
            if (!other.startsWith("@")) r.go(r.hrefOf(other), leafOf(other));
          }}
      onplace={r === null
        ? null
        : (to, how, label) => {
            const hash = to === "" ? r.hrefOf(schema.path) : r.placeHrefOf(schema.path, to);
            if (how === "push") r.go(hash, label);
            else r.replace(hash);
          }}
      onback={r === null ? null : () => history.back()}
    />
  {/await}
{:else if route === "doc"}
  <!-- An XML or JSON file (the XML view, 2026-11-23): its view and its worker load with the first document opened. -->
  {@const doc = drawn}
  {@const r = repo}
  {@const json = renderer?.id === "json"}
  {#await docViewModule() then { default: TfDocView }}
    <TfDocView
      text={doc.text}
      path={doc.path}
      kind={json ? (isDelimited(doc.path) ? "jsonl" : "json") : "xml"}
      jq={!json || r === null ? null : { open: () => r.query(doc) }}
      onpipeline={!json || r === null || !r.asRead(doc) ? null : (filter) => r.pipeline(doc.path, filter)}
      repo={r?.id ?? ""}
      sha={r !== null && r.asRead(doc) ? r.file.sha : null}
      {render}
      line={r !== null && r.path === doc.path ? r.line : null}
      files={r?.files ?? null}
      schema={r?.schema ?? null}
      values={r === null || r.values === null || !r.asRead(doc) ? null : r.values(doc.path)}
    />
  {/await}
{:else if route === "markdown"}
  <div class="tf-rendered" bind:this={rendered}><TfMarkdown text={drawn.text} base={repo === null ? "" : parentOf(repo.file.path)} fileLink={repo?.fileLink ?? null} {render} image={repo?.images ?? null} imagesOf={repo?.entity ?? ""} /></div>
  {#if find}<TfFindBar root={() => rendered} within="in this file" />{/if}
{:else if route === "html"}
  <!-- A page (ADR-0046): sanitized, in the frame that runs no script and makes no request. -->
  <TfHtmlPage html={drawn.text} title="{leafOf(drawn.path)}, drawn as a page" {frame} {room} />
{:else if route === "diagram"}
  <TfDiagram engine={renderer!.id as DiagramEngine} code={drawn.text} {render} />
{:else if route === "command"}
  <div class="tf-rendered" bind:this={rendered}><TfCommandOutput renderer={renderer!} text={drawn.text} {commands} /></div>
  {#if find}<TfFindBar root={() => rendered} within="in this file" />{/if}
{:else if route === "lines"}
  <TfLines text={drawn.text} path={drawn.path} {render} line={repo?.line ?? null} bind:typed />
{/if}
