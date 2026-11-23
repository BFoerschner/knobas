<!--
  Test fixture for `TfPage.test.svelte.ts` (#2247) — nothing else imports it,
  so it is tree-shaken out of the app bundle.

  A page that writes down each instance it starts as and each read it makes:
  one when it mounts, and one each time `syncRevision` moves after that, as a
  role view's board does. It draws its `visible` prop and what `pageVisible()`
  answers inside it, so a test sees both.
-->
<script lang="ts" module>
  export interface PageLog {
    mounts: string[];
    reads: string[];
    /** Each time anything reading `syncRevision` ran again, moved or not: a page's read effect waking. */
    wakes?: string[];
    /** Each time `syncAsked` moved: the person's own *Sync now* ended, which sorts a held list again. */
    asks?: string[];
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { pageVisible } from "./kept-pages.svelte";

  let {
    name,
    visible,
    log,
    syncRevision = 0,
    syncAsked = 0,
  }: { name: string; visible: boolean; log: PageLog; syncRevision?: number; syncAsked?: number } = $props();

  const shown = pageVisible();
  const born = untrack(() => name);
  const pages = untrack(() => log);
  pages.mounts.push(born);
  pages.reads.push(born);

  let seen = untrack(() => syncRevision);
  $effect(() => {
    const at = syncRevision;
    pages.wakes?.push(born);
    if (at === seen) return;
    seen = at;
    pages.reads.push(born);
  });
  let asked = untrack(() => syncAsked);
  $effect(() => {
    const at = syncAsked;
    if (at === asked) return;
    asked = at;
    pages.asks?.push(born);
  });
</script>

<p class="fixture-page" data-page={name} data-visible={String(visible)} data-shown={String(shown())}>{name}</p>
