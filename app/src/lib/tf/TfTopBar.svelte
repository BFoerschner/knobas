<!--
  The ticket-first top bar (#809, R-1.1, R-1.3, R-1.4): the app name, the main
  navigation with its count badges, *Search*, the timer, the Settings cog and
  the theme toggle, left to right as prototype-6 draws them.

  A badge is drawn only once its read has answered and only above zero.
  Prototype-6 always has something to count; an empty inbox drawing `0` is a
  place for the eye to keep checking, which is the rooms strip's rule too.
-->
<script lang="ts">
  import type { Inbox } from "../inbox/inbox.svelte";
  import type { Timer } from "../shell/timer.svelte";
  import { targetReading } from "../shell/timer";
  import type { Badges } from "./badges.svelte";
  import { jobHash, NAV, SECTION_NAME, sectionHash, type Section } from "./route";

  let {
    current,
    job,
    jobAt = null,
    inbox,
    badges,
    timer,
    onsearch,
    ontimer,
    ontheme,
  }: {
    /** The section the address lights, or `null`. */
    current: Section | null;
    /** The job the Job tab opens: the chosen job (spec #2227), else the timer's ticket. */
    job: string | null;
    /** That job's entity id (#1525), which the tab's address names; its key when omitted. */
    jobAt?: string | null;
    inbox: Pick<Inbox, "count">;
    badges: Pick<Badges, "builds" | "monitors" | "proposals">;
    timer: Pick<Timer, "current" | "elapsed">;
    onsearch: () => void;
    /** Stop the timer into the worklog draft, or start one: the shell's verb. */
    ontimer: (() => void) | undefined;
    ontheme: () => void;
  } = $props();

  /** A badge's number and tone, or `null` where none is drawn. */
  function badge(section: Section): { n: number; err: boolean } | null {
    const n =
      section === "inbox"
        ? inbox.count
        : section === "builds"
          ? badges.builds
          : section === "monitors"
            ? badges.monitors
            : section === "review"
              ? badges.proposals
              : null;
    if (n === null || n <= 0) return null;
    return { n, err: section === "builds" || section === "monitors" };
  }

  function hrefOf(section: Section): string {
    return section === "job" ? jobHash(jobAt ?? job) : sectionHash(section);
  }

  /** Prototype-6 draws the clock as `H:MM:SS` from the first second. */
  const clock = $derived.by(() => {
    const parts = (timer.elapsed ?? "0:00").split(":");
    return parts.length === 2 ? `0:${parts[0]!.padStart(2, "0")}:${parts[1]}` : parts.join(":");
  });
  /** What the clock runs on, as the chip names it: a ticket key, a label, an asset id. */
  const clockOn = $derived(timer.current === null ? null : targetReading(timer.current.target));
</script>

<header class="tf-top">
  <span class="tf-brand">knobas</span>
  <nav class="tf-nav" aria-label="Main">
    {#each NAV as section (section)}
      {@const count = badge(section)}
      <a href={hrefOf(section)} aria-current={section === current ? "page" : undefined}>
        {SECTION_NAME[section]}
        {#if section === "job" && job !== null}
          <span class="tf-navkey">{job}</span>
        {/if}
        {#if count}
          <span class="tf-count {count.err ? 'tf-err' : ''}">{count.n}</span>
        {/if}
      </a>
    {/each}
  </nav>
  <span class="tf-spacer"></span>
  <button class="tf-search" aria-label="Search everything" onclick={onsearch}>
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
      <circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" />
    </svg>
    <span class="tf-label">Search</span><kbd>⌘K</kbd>
  </button>
  {#if clockOn !== null}
    <button
      class="tf-timer tf-card"
      title="Timer (⌘T)"
      aria-label="Timer on {clockOn}, {clock}. Stop and log"
      disabled={ontimer === undefined}
      onclick={() => ontimer?.()}
    >
      <span class="tf-timer-key" class:tf-mono={timer.current?.target.kind === "entity"} title={clockOn}>{clockOn}</span>
      <span class="tf-timer-t">{clock}</span>
      <span class="tf-timer-stop" title="Stop and log"><i></i></span>
    </button>
  {:else}
    <button
      class="tf-timer tf-off"
      title="Timer (⌘T)"
      disabled={ontimer === undefined}
      onclick={() => ontimer?.()}
    >
      <span>No timer</span>
      <span class="tf-timer-stop" aria-hidden="true">▶</span>
    </button>
  {/if}
  <a
    class="tf-iconbtn"
    href="#/settings"
    aria-label="Settings"
    title="Settings (⌘,)"
    aria-current={current === "settings" ? "page" : undefined}
  >
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" aria-hidden="true">
      <circle cx="8" cy="8" r="5.6" stroke-width="2.6" stroke-dasharray="2.2 2.2" />
      <circle cx="8" cy="8" r="3.9" stroke-width="1.6" />
      <circle cx="8" cy="8" r="1.3" stroke-width="1.4" />
    </svg>
  </a>
  <button class="tf-iconbtn" aria-label="Toggle colour theme" onclick={ontheme}>
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1zm0 12.5v-11a5.5 5.5 0 0 1 0 11z" />
    </svg>
  </button>
</header>
