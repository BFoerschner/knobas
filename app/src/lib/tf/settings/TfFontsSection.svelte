<!--
  Settings › Fonts (2026-11-18, Björn: "a settings pane for being able to switch all the different fonts, not just
  the code one and also being able to change their individual font sizes (relative, since some fonts turn out smaller
  etc.)"; `docs/decisions/2026-11-18-every-face-is-yours.md`). Beside Appearance, which keeps Theme, Zoom and the
  state colours. Needs no database: the choices are this machine's, in `localStorage` (`fonts.svelte.ts`).

  At the top, the proof: one word in each role's face at one nominal size times the role's own, on one baseline, with
  a rule at the bundled interface face's x-height across them. Evening a face out is making its lower case meet that
  rule, so the proof shows at a glance what the sizes below do. Then the four roles, each a row (`TfFontRole.svelte`).

  Last, the faces knobas brings along, each with its licence's full text a click away (2026-11-20,
  `fonts/bundled-faces.ts`): the SIL Open Font License asks that its text go with every copy of the font.
-->
<script lang="ts">
  import type { InstalledFont } from "../../ipc/app";
  import { ROLES } from "../fonts.svelte";
  import { BUNDLED_FACES, LICENCE_NAME, type BundledFace } from "../fonts/bundled-faces";
  import TfFontRole from "./TfFontRole.svelte";

  let { installedFonts }: { installedFonts?: () => Promise<InstalledFont[]> } = $props();

  /** Each licence's text once opened; read from its own chunk the first time. */
  const texts = $state<Record<string, string>>({});
  function opened(face: BundledFace, event: Event): void {
    if (!(event.currentTarget as HTMLDetailsElement).open || face.folder in texts) return;
    void face.licence().then((text) => (texts[face.folder] = text));
  }
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
  <figcaption>Interface, Headings, Reading and Code on one line. The upper rule is Inter's x-height.</figcaption>
</figure>
{#each ROLES as role (role)}
  <TfFontRole {role} {...installedFonts ? { installedFonts } : {}} />
{/each}
<section class="tf-stg-sec tf-fr-lics" aria-labelledby="tf-fr-lics-h">
  <h2 id="tf-fr-lics-h">Fonts that come with knobas</h2>
  <p class="tf-fr-lics-note">Free to use, share and bundle under the {LICENCE_NAME}.</p>
  {#each BUNDLED_FACES as face (face.folder)}
    <details class="tf-fr-lic" ontoggle={(event) => opened(face, event)}>
      <summary
        ><b>{face.name}</b> <span class="tf-dim tf-num">{face.version}</span><span class="tf-fr-lic-role">{face.roles}</span><span
          class="tf-dim">© {face.by}</span
        ></summary
      >
      <pre class="tf-fr-lic-text">{texts[face.folder] ?? ""}</pre>
    </details>
  {/each}
</section>
