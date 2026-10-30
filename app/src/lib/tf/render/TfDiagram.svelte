<!--
  A diagram file in the file viewer (#2258, spec #2234 step 5): `.mmd` and
  `.mermaid` drawn by Mermaid, sanitized, in a sandboxed frame sized to it,
  and drawn again on a theme switch (`render/diagram.ts`). A diagram that
  does not parse shows the parser's error above its source.
-->
<script lang="ts">
  import { drawDiagram, type DiagramEngine } from "../../render/diagram";
  import { renderPort, type RenderPort } from "../../render/port";

  let {
    engine,
    code,
    render = renderPort(),
  }: {
    engine: DiagramEngine;
    /** The diagram's source. */
    code: string;
    /** The engines (#2254): the app's, a fake in tests. */
    render?: RenderPort;
  } = $props();

  function paint(node: HTMLElement) {
    node.replaceChildren();
    return drawDiagram(engine, code, node, render);
  }
</script>

<div class="tf-fdiagram" {@attach paint}></div>
