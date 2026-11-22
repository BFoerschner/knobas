<!--
  An entry's one-time code (ADR-0056): made on this machine from the key the last sync kept in the keychain, so it reads
  nothing from the source, asks no usage comment and works offline. Set in the job key's type, the one loud thing on
  the page, in two groups of three. Asked when the entry opens and once at each period's turn; the comb counts the
  period down.
-->
<script lang="ts">
  import type { CodeAnswer, PwEntry } from "../../ipc/passwords";
  import TfPwComb from "./TfPwComb.svelte";
  import { clip, clipped } from "./clip.svelte";
  import type { PasswordPorts } from "./ports";

  let { entry, word, ports, visible = true }: { entry: PwEntry; word: string; ports: PasswordPorts; visible?: boolean } = $props();

  let answer = $state<CodeAnswer | null>(null);
  let until = $state(0);
  let failed = $state<string | null>(null);
  let turn: ReturnType<typeof setTimeout> | null = null;
  /** Bumped when the row is let go or its entry changes: an answer for an older ask schedules nothing. */
  let generation = 0;

  async function ask(): Promise<void> {
    if (turn !== null) clearTimeout(turn);
    turn = null;
    const mine = generation;
    try {
      const next = await ports.code(entry.id);
      if (mine !== generation) return;
      answer = next;
      failed = null;
      if (next.kind === "code") {
        until = Date.now() + next.left_ms;
        turn = setTimeout(() => void ask(), next.left_ms + 50);
      }
    } catch (cause) {
      failed = String((cause as { message?: string })?.message ?? cause);
    }
  }

  $effect(() => {
    void entry.id;
    if (!visible) return;
    void ask();
    return () => {
      generation++;
      if (turn !== null) clearTimeout(turn);
      turn = null;
    };

  });

  const copied = $derived(clip().entity_id === entry.id && clip().what === "one-time code" && clip().until !== null);
  const blocked = $derived(answer?.kind === "hidden");

  export async function copy(): Promise<void> {
    if (answer?.kind !== "code") return;
    const next = await ports.copyCode(entry.id);
    if (next.kind === "code") clipped({ what: "one-time code", entity_id: entry.id, until: new Date(Date.now() + 30_000).toISOString() });
  }
</script>

<div class="tf-pw-f tf-pw-otp">
  <div class="tf-pw-fl">One-time code</div>
  <div class="tf-pw-fv">
    {#if answer?.kind === "code"}
      <span class="tf-pw-code" aria-label="One-time code {answer.code}">{answer.code.slice(0, Math.ceil(answer.digits / 2))}<i>{answer.code.slice(Math.ceil(answer.digits / 2))}</i></span>
      <span class="tf-pw-otpleft">
        <TfPwComb {until} span={answer.period} size="lg" label={["New code in ", ""]} />
      </span>
    {:else}
      <span class="tf-pw-code tf-pw-cover" role="img" aria-label="Hidden"></span>
    {/if}
  </div>
  <div class="tf-pw-fa">
    <button type="button" class="tf-btn" aria-label="Copy the code" aria-keyshortcuts="O" disabled={answer?.kind !== "code"} onclick={() => void copy()}>Copy<kbd>O</kbd></button>
  </div>
  <div class="tf-pw-fh">
    {#if failed}<span class="tf-stg-errt">{failed}</span>
    {:else if blocked}Hidden until your request is approved.
    {:else if answer?.kind === "needs_sync"}Sync now to make codes here.
    {:else if answer?.kind === "unreadable"}This entry's code key is in a form knobas cannot read.
    {:else if copied}<span class="tf-pw-timeleft"
        ><TfPwComb until={clip().until ? Date.parse(clip().until!) : 0} size="sm" label={["On the clipboard. It clears in ", "."]} /></span
      >
      Nothing was read from {word}{entry.comment.read ? ", so no comment was asked" : ""}.
    {:else}Made on this machine. Works offline.{/if}
  </div>
</div>
