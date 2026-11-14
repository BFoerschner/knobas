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
  import { keyHint } from "../shell/platform";
  import type { Badges } from "./badges.svelte";
  import { NAV, SECTION_NAME, sectionHash, type Section } from "./route";

  type NavSection = Exclude<Section, "job">;

  let {
    current,
    inbox,
    badges,
    timer,
    job = null,
    onsearch,
    ontimer,
    ontheme,
    navHref = sectionHash,
    onnav,
  }: {
    /** The section the address lights, or `null`. */
    current: Section | null;
    inbox: Pick<Inbox, "count">;
    badges: Pick<Badges, "builds" | "monitors">;
    timer: Pick<Timer, "current" | "elapsed">;
    /** The chosen job, as its key: what *Start* starts on, so the bar always names the job. */
    job?: string | null;
    onsearch: () => void;
    /** Stop the timer into the worklog draft, or start one: the shell's verb. */
    ontimer: (() => void) | undefined;
    ontheme: () => void;
    /** Where a nav entry goes: the section as it was left; its own page when left out. */
    navHref?: (section: NavSection) => string;
    /** A plain press on a nav entry, which the shell makes, and whether a person made it; a plain link when left out. */
    onnav?: (section: NavSection, href: string, person: boolean) => void;
  } = $props();

  function press(event: MouseEvent, section: NavSection) {
    if (onnav === undefined || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    onnav(section, navHref(section), event.isTrusted);
  }

  /** A badge's number and tone, or `null` where none is drawn. */
  function badge(section: Section): { n: number; err: boolean } | null {
    const n =
      section === "inbox"
        ? inbox.count
        : section === "builds"
          ? badges.builds
          : section === "monitors"
            ? badges.monitors
            : null;
    if (n === null || n <= 0) return null;
    return { n, err: section === "builds" || section === "monitors" };
  }

  /** Prototype-6 draws the clock as `H:MM:SS` from the first second. */
  const clock = $derived.by(() => {
    const parts = (timer.elapsed ?? "0:00").split(":");
    return parts.length === 2 ? `0:${parts[0]!.padStart(2, "0")}:${parts[1]}` : parts.join(":");
  });
  /** What the clock runs on, as the chip names it: a ticket key, a label, an asset id. */
  const clockOn = $derived(timer.current === null ? null : targetReading(timer.current.target));

  /** Below 1040 px the nav scrolls: the page you are on stays in view when you get there. */
  function keepCurrentInView(nav: HTMLElement) {
    void current;
    const at = nav.querySelector('[aria-current="page"]');
    if (at === null) return;
    const bring = () => at.scrollIntoView?.({ inline: "nearest", block: "nearest" });
    // Once the page has painted: measured in the press's task, it laid out the page the press had just shown there
    // (PR #2479: 63 ms of a quarter-speed main thread on a return to Builds), and again in the frame.
    if (typeof requestAnimationFrame !== "function") return bring();
    let later = 0;
    const frame = requestAnimationFrame(() => (later = window.setTimeout(bring, 0)));
    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(later);
    };
  }
</script>

<header class="tf-top">
  <span class="tf-brand">knobas</span>
  <nav class="tf-nav" aria-label="Main" {@attach keepCurrentInView}>
    {#each NAV as section (section)}
      {@const count = badge(section)}
      <!-- `data-section` names the entry whatever place its link holds: what the speed checks press. -->
      <a href={navHref(section)} data-section={section} aria-current={section === current ? "page" : undefined} onclick={(event) => press(event, section)}>
        {SECTION_NAME[section]}
        {#if count}
          <span class="tf-count {count.err ? 'tf-err' : ''}">{count.n}</span>
        {/if}
      </a>
    {/each}
  </nav>
  <span class="tf-spacer"></span>
  <button class="tf-search" aria-label="Search this section" onclick={onsearch}>
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
      <circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" />
    </svg>
    <span class="tf-label">Search</span><kbd>{keyHint("⌘/")}</kbd>
  </button>
  {#if clockOn !== null}
    <button
      class="tf-timer tf-card"
      title={keyHint("Timer (⌘T)")}
      aria-label="Timer on {clockOn}, {clock}. Stop and log…"
      disabled={ontimer === undefined}
      onclick={() => ontimer?.()}
    >
      <span class="tf-timer-key" class:tf-mono={timer.current?.target.kind === "entity"} title={clockOn}>{clockOn}</span>
      <span class="tf-timer-t">{clock}</span>
      <span class="tf-timer-stop" title="Stop and log…"><i></i></span>
    </button>
  {:else}
    <button
      class="tf-timer tf-off"
      title={keyHint(job === null ? "Timer (⌘T)" : `Start the timer on ${job} (⌘T)`)}
      disabled={ontimer === undefined}
      onclick={() => ontimer?.()}
    >
      <span>Start{#if job !== null}{" "}<span class="tf-timer-key tf-mono">{job}</span>{:else}{" "}timer{/if}</span>
      <span class="tf-timer-stop" aria-hidden="true">▶</span>
    </button>
  {/if}
  <a
    class="tf-iconbtn"
    href={navHref("settings")}
    aria-label="Settings"
    title={keyHint("Settings (⌘,)")}
    aria-current={current === "settings" ? "page" : undefined}
    onclick={(event) => press(event, "settings")}
  >
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" aria-hidden="true">
      <circle cx="8" cy="8" r="5.6" stroke-width="2.6" stroke-dasharray="2.2 2.2" />
      <circle cx="8" cy="8" r="3.9" stroke-width="1.6" />
      <circle cx="8" cy="8" r="1.3" stroke-width="1.4" />
    </svg>
  </a>
  <button class="tf-iconbtn" aria-label="Toggle colour theme" title="Toggle colour theme" onclick={ontheme}>
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1zm0 12.5v-11a5.5 5.5 0 0 1 0 11z" />
    </svg>
  </button>
</header>
