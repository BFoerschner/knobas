<!--
  Settings › Appearance › Code colours' preview (ADR-0051): the app's own code areas, drawn with their own classes, so what a theme does
  here is what it does everywhere. Code and diff (a file view of eleven Java lines that hold every code role, and a
  YAML change with a changed word, an added line with a comment and quiet context), a Docs code block, and Markdown
  source. The code is highlighted through the port like any other text, so it is the engine's answer, not a picture.

  The theme shown is the one in the editor, its colours set on this box, on the surfaces of the window theme drawn for
  its side (`scopeValues`, ADR-0055), so a dark code theme is shown on the dark window theme while the window is light.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import type { LineTokens, RenderPort } from "../../render/port";
  import { scopeValues } from "../themes.svelte";
  import type { Colours } from "../theme/app-derive";
  import { cssVars, marked } from "../theme/paint";
  import type { Theme, ThemeKind } from "../theme/themes";

  let { theme, drawnSide, render, windowTheme, colours }: { theme: Theme; drawnSide: ThemeKind; render: RenderPort; windowTheme: Theme; colours: Colours } = $props();

  type Tab = "code" | "docs" | "markdown";
  let tab = $state<Tab>("code");
  const TABS: readonly (readonly [Tab, string])[] = [
    ["code", "Code and diff"],
    ["docs", "Docs"],
    ["markdown", "Markdown"],
  ];

  const JAVA = [
    "@Override",
    "public Receipt settle(Batch batch) {",
    "    // A batch is retried three times, then parked.",
    "    int tries = 0;",
    "    while (tries++ < MAX_TRIES) {",
    "        var sent = gateway.send(batch.id);",
    "        if (sent.ok()) return sent.receipt();",
    '        log.warn("retry {} of {}\\n", tries, MAX_TRIES);',
    "    }",
    "    throw new ParkedException(batch);",
    "}",
  ].join("\n");
  const YAML_OLD = ["  services:", "    gateway:", "      image: tidewater/gateway:3.2.8", "      environment:", "        RETRY_LIMIT: 3", "      restart: unless-stopped"].join("\n");
  const YAML_NEW = [
    "  services:",
    "    gateway:",
    "      image: tidewater/gateway:3.2.9",
    "      environment:",
    "        RETRY_LIMIT: 3",
    '        RETRY_BACKOFF: "250ms"  # doubles each try',
    "      restart: unless-stopped",
  ].join("\n");
  const SHELL = ["# Run the gateway against the local queue", "export RETRY_LIMIT=3", "docker compose up gateway --wait", "curl -s \"http://localhost:8080/health\" | jq '.status'"].join("\n");
  const MARKDOWN = [
    "## Running it",
    "",
    "Start the queue first, then **the gateway**. See",
    "[the retry notes](https://wiki.example/retry) for *why* it parks.",
    "",
    "- `RETRY_LIMIT` caps the tries",
  ].join("\n");

  /** The diff: its rows, each a side's line (0-based), its numbers and, for the changed image, the marked word. */
  const DIFF: readonly { kind: "context" | "add" | "del"; old: number | null; new: number | null; mark?: [number, number] }[] = [
    { kind: "context", old: 0, new: 0 },
    { kind: "context", old: 1, new: 1 },
    { kind: "del", old: 2, new: null, mark: [31, 36] },
    { kind: "add", old: null, new: 2, mark: [31, 36] },
    { kind: "context", old: 3, new: 3 },
    { kind: "context", old: 4, new: 4 },
    { kind: "add", old: null, new: 5 },
    { kind: "context", old: 5, new: 6 },
  ];
  const FIRST = 12;
  const SIGN = { context: " ", add: "+", del: "−" } as const;

  const texts = { java: JAVA, old: YAML_OLD, new: YAML_NEW, shell: SHELL, markdown: MARKDOWN } as const;
  type Key = keyof typeof texts;
  const lines: Record<Key, string[]> = {
    java: JAVA.split("\n"),
    old: YAML_OLD.split("\n"),
    new: YAML_NEW.split("\n"),
    shell: SHELL.split("\n"),
    markdown: MARKDOWN.split("\n"),
  };
  const tokens = $state<Record<Key, (LineTokens | undefined)[]>>({ java: [], old: [], new: [], shell: [], markdown: [] });

  // Each text is a job held for as long as the preview is drawn: the port answers it again when the highlighter
  // changes, and every answer is written over the lines it is for.
  const jobs = (
    [
      ["java", { path: "RetryQueue.java" }],
      ["old", { path: "deploy/compose.yaml" }],
      ["new", { path: "deploy/compose.yaml" }],
      ["shell", { lang: "bash" }],
      ["markdown", { path: "README.md" }],
    ] as const
  ).map(([key, where]) =>
    render.highlight(
      { text: texts[key], ...where },
      {
        onLines: (from, got) => {
          const next = [...tokens[key]];
          got.forEach((line, i) => (next[from + i] = line));
          tokens[key] = next;
        },
      },
    ),
  );
  onDestroy(() => jobs.forEach((job) => job.cancel()));

  const other = $derived(theme.kind !== drawnSide);
</script>

{#snippet code(key: Key, n: number, from?: number, to?: number)}{#each marked(lines[key][n] ?? "", tokens[key][n], from, to) as piece, i (i)}{#if piece.mark}<span class="tf-wd">{#if piece.kind === ""}{piece.text}{:else}<span class="tf-hl-{piece.kind}">{piece.text}</span>{/if}</span>{:else if piece.kind === ""}{piece.text}{:else}<span class="tf-hl-{piece.kind}">{piece.text}</span>{/if}{/each}{/snippet}

<div class="tf-thm-pre" {@attach cssVars(() => scopeValues(windowTheme, theme, colours))}>
  <div class="tf-thm-preh">
    <div class="tf-seg" role="group" aria-label="Preview">
      {#each TABS as [key, label] (key)}
        <button type="button" aria-pressed={tab === key} onclick={() => (tab = key)}>{label}</button>
      {/each}
    </div>
    <span class="tf-dim">{other ? `Shown as it draws while the window is ${theme.kind}.` : "Drawn as every code area draws it. A change shows at once."}</span>
  </div>
  {#if tab === "code"}
    <div class="tf-fileview">
      <header><span class="tf-p"><span class="tf-pdir">payout/retry/</span>RetryQueue.java</span></header>
      <pre class="tf-code">{#each lines.java as _, n (n)}<div class="tf-l"><span>{41 + n}</span><span>{@render code("java", n)}</span></div>{/each}</pre>
    </div>
    <div class="tf-diff">
      <div class="tf-fh"><span class="tf-fh-id"><span class="tf-path tf-mono">deploy/compose.yaml</span><span class="tf-dim tf-thm-stat">+2 −1</span></span></div>
      <div class="tf-dbody">
        {#each DIFF as row, i (i)}
          {@const side = row.kind === "del" ? "old" : "new"}
          {@const n = (side === "old" ? row.old : row.new) ?? 0}
          <div class="tf-ln" class:tf-add={row.kind === "add"} class:tf-del={row.kind === "del"}>
            <span class="tf-no">{row.old === null ? "" : FIRST + row.old}</span><span class="tf-no">{row.new === null ? "" : FIRST + row.new}</span><span
              >{SIGN[row.kind]} {@render code(side, n, row.mark?.[0], row.mark?.[1])}</span
            >
          </div>
        {/each}
      </div>
    </div>
  {:else if tab === "docs"}
    <div class="tf-doc tf-thm-doc">
      <p>Start the gateway on the local queue before a release run.</p>
      <div class="tf-pre">
        <div class="tf-pre-bar"><span>bash</span></div>
        <pre><code>{#each lines.shell as _, n (n)}{#if n > 0}{"\n"}{/if}{@render code("shell", n)}{/each}</code></pre>
      </div>
    </div>
  {:else}
    <div class="tf-fileview">
      <header><span class="tf-p">README.md</span></header>
      <pre class="tf-code">{#each lines.markdown as _, n (n)}<div class="tf-l"><span>{1 + n}</span><span>{@render code("markdown", n)}{#if lines.markdown[n] === ""}{" "}{/if}</span></div>{/each}</pre>
    </div>
  {/if}
</div>
