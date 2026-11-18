<!--
  Settings › Fonts (2026-11-18, Björn: "a settings pane for being able to switch all the different fonts, not just
  the code one and also being able to change their individual font sizes (relative, since some fonts turn out smaller
  etc.)"; `docs/decisions/2026-11-18-every-face-is-yours.md`). Beside Appearance, which keeps Theme, Zoom and the
  state colours. Needs no database: the choices are this machine's, in `localStorage` (`fonts.svelte.ts`).

  At the top, the proof: one word in each role's face at one nominal size times the role's own, on one baseline, with
  a rule at the bundled interface face's x-height across them. Evening a face out is making its lower case meet that
  rule, so the proof shows at a glance what the sizes below do. Then the four roles, each a row (`TfFontRole.svelte`).
-->
<script lang="ts">
  import type { InstalledFont } from "../../ipc/app";
  import { ROLES } from "../fonts.svelte";
  import TfFontRole from "./TfFontRole.svelte";

  let { installedFonts }: { installedFonts?: () => Promise<InstalledFont[]> } = $props();
</script>

<header class="tf-stg-h">
  <h1>Fonts</h1>
  <p>
    Size evens a face out against the others; to make everything bigger, use Zoom in Appearance. A font this computer
    does not have draws in the one that comes with knobas. Kept on this computer.
  </p>
</header>
<figure class="tf-fr-proof" aria-label="The four faces on one line">
  <div class="tf-fr-proofline" aria-hidden="true">
    <span class="tf-fr-guide"></span>
    <span class="tf-fr-pw tf-fr-pw-ui">Review</span>
    <span class="tf-fr-pw tf-fr-pw-head">Sprint 14</span>
    <span class="tf-fr-pw tf-fr-pw-read">Release notes</span>
    <span class="tf-fr-pw tf-fr-pw-code">git rebase</span>
  </div>
  <figcaption>Interface, Headings, Reading and Code on one line. The upper rule is Archivo's x-height.</figcaption>
</figure>
{#each ROLES as role (role)}
  <TfFontRole {role} {...installedFonts ? { installedFonts } : {}} />
{/each}
