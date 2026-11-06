<!--
  The app's one dialog shell (#830, review of #2238): prototype-6's `.dialog`,
  a header, a body and a footer of buttons over a scrim. A dialog of its own
  width passes `className`; the asset tree's dialogs ask for a header Close.
  A dialog that acts on what it shows puts that action on its title line
  (`actions`: the file viewer's *Download*, #2415); with a Close beside it,
  the two are one cluster centred on the title block, a rule between them.
  A title with markup in it (the worklog draft's ticket key in mono) is a
  snippet. A dialog whose body is a form keeps the `<form>` in the body and
  points its footer's submit at it with `form=` (`TfApplySet`, `TfBlockDialog`).

  It registers itself with the shell's layer stack while it is open, so the
  window's `Esc` closes it as rung 1 of the ladder (`escape.ts`) even when focus
  is outside it, and history stays put under it (#1086, #1527). An `Esc` pressed
  inside it closes it here and stops there, so the shell never unwinds a second
  ladder on the same press. `Tab` stays inside it.

  **Arrival focus** (design system, Dialogs): its first field, else a control in
  its body that isn't a button or a link, else the ×. Never a button in the body:
  `Enter` on arrival must not send what the dialog is asking about. Focus returns
  to what opened it when it closes, and a control removed while it held focus
  (a pick, a row that went) hands it back here rather than to the page.

  **A press that closes it holds for a moment** (`still-press.ts`): what it covered is now
  under the pointer and focus is back on its opener, so the second press of a double press
  lands on nothing. Only after a person's press in it: one that goes with its page, or by itself,
  has no second press to catch, and neither has `Esc`, where `Enter` straight after reopening it
  is a person changing their mind.

  **The scrim.** A dialog with a footer has something to lose, so a click on the
  scrim does nothing and leaves focus where it was. A viewer has no footer, and the
  launcher's only names keys (`scrimCloses`): that click closes them.

  **The missing line.** `why` is what a disabled primary waits for (Controls, the
  enabled rule): this draws it, just left of *Cancel*, and the gate refuses a
  second markup for it.

  **`Enter` on a radio presses the footer's primary**, so a keyboard reaches a harmless
  write without tabbing past *Cancel* (the key table, F-240). A danger primary or one marked
  `data-no-enter` (*Merge*) is not reached this way, and neither is one that is off. `Enter` on
  a tick sends nothing: it is one more thing to decide. **⌘Enter presses it too, from anywhere in
  the dialog**, so every composer in a dialog sends the same way (the key table).

  Deliberately not a native `<dialog>`: `showModal()` puts the element in the
  top layer, where `::backdrop`, not `.tf-modal`, draws the dim. What a native
  dialog gives is focus trapping, Escape and focus restore, so those three are
  written out here and tested in `TfDialog.test.svelte.ts`.
-->
<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  import { isSendKey } from "../inline-editor";
  import { layers as sharedLayers, type Layer, type Layers } from "../layers.svelte";
  import { holdStillPress } from "../still-press";
  import TfKeyed from "../TfKeyed.svelte";

  let {
    title,
    subtitle = null,
    onclose,
    layers = sharedLayers,
    body,
    footer,
    className = "",
    closeButton = true,
    actions,
    why = null,
    scrimCloses = undefined,
  }: {
    /** Text, or a snippet when the title holds markup (#2346); either way it names the dialog. */
    title: string | Snippet;
    /** A line under the title, in the dialog's sub tone (the write queue's counts, #1425). */
    subtitle?: string | null;
    onclose: () => void;
    layers?: Layers;
    body: Snippet;
    footer?: Snippet | undefined;
    /** A class beside `tf-dialog`, for a dialog of its own size (the view editor's, the import dialog's `tf-dwide`). */
    className?: string;
    /** The Close icon button on the right of the title line: every dialog has one (design system, Dialogs). */
    closeButton?: boolean;
    /** Buttons on the right of the title line, before the Close: the title line is the dialog's header. */
    actions?: Snippet;
    /** What the footer's disabled primary waits for, in words; `null` while it can be pressed or is only sending. */
    why?: string | null;
    /** Whether a click on the scrim closes it: a dialog with no footer does. The launcher's footer only names keys, so it says so. */
    scrimCloses?: boolean | undefined;
  } = $props();

  const leavesOnScrim = $derived(scrimCloses ?? footer === undefined);

  const uid = $props.id();
  const titleId = `tf-dlg-${uid}`;
  let dialog = $state<HTMLDivElement | null>(null);

  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  /** What matches `selector` and can take focus: a field in a closed fold can't, and `focus()` on it does nothing. */
  function reachable(selector: string): HTMLElement[] {
    if (!dialog) return [];
    return [...dialog.querySelectorAll<HTMLElement>(selector)].filter((element) => !element.closest("details:not([open])"));
  }

  /** Everything inside the dialog a Tab can reach, in document order. */
  function focusable(): HTMLElement[] {
    return reachable(FOCUSABLE);
  }

  const FIELD = 'input:not([disabled], [type="radio"], [type="checkbox"], [type="hidden"]), textarea:not([disabled]), select:not([disabled])';
  /** A tick, a radio or a scroller the page made focusable: nothing a press on it sends. */
  const NOT_A_BUTTON = 'input:not([disabled], [type="hidden"]), [tabindex]:not([tabindex="-1"], button, a, [role="button"], [role="option"])';

  /** Where focus goes on opening, and when what held it is removed. */
  function arrival(): HTMLElement | null {
    return reachable(`.tf-body :is(${FIELD})`)[0] ?? reachable(`.tf-body :is(${NOT_A_BUTTON})`)[0] ?? reachable(".tf-dclose")[0] ?? dialog;
  }

  /** This dialog's entry on the layer stack. */
  const layer: Layer = {
    kind: "dialog",
    close: () => {
      steppedOut = true;
      onclose();
    },
  };
  /** Closed by `Esc`, which holds nothing. */
  let steppedOut = false;
  /** A person pressed in it, by pointer or `Enter`. A write may answer long after, so this stands until it closes. */
  let pressed = false;
  const notePress = (event: Event) => {
    if (event.isTrusted && (!(event instanceof KeyboardEvent) || event.key === "Enter")) pressed = true;
  };

  $effect(() => {
    const restoreTo = document.activeElement;
    // Untracked: `add` reads the stack it writes, and this effect must run once.
    const off = untrack(() => layers.add(layer));
    // The header's Close comes first in document order, and a reader who opened a dialog to type would type into nothing (#1431).
    arrival()?.focus();
    // A control removed while focused drops focus on the body, and `Tab` from there walks the page behind (F-030).
    const fallen = new MutationObserver(() => {
      if (layers.open.at(-1) !== layer) return;
      if (document.activeElement === null || document.activeElement === document.body) arrival()?.focus({ preventScroll: true });
    });
    if (dialog) fallen.observe(dialog, { childList: true, subtree: true });
    return () => {
      fallen.disconnect();
      untrack(off);
      if (restoreTo instanceof HTMLElement && document.contains(restoreTo)) restoreTo.focus();
      if (pressed && !steppedOut) holdStillPress();
    };
  });

  /** `Enter` on a radio and ⌘Enter: the footer's primary, if it can be pressed and is safe to reach by a key that also means *go on*. */
  function pressPrimary() {
    const primary = dialog?.querySelector<HTMLButtonElement>("footer .tf-primary");
    if (!primary || primary.disabled || primary.getAttribute("aria-disabled") === "true") return;
    if (primary.matches(".tf-danger, [data-no-enter]")) return;
    primary.click();
  }

  function onkeydown(event: KeyboardEvent) {
    // A field that takes the key for itself says so (the launcher's box opens its action chain on `Tab`, #1422).
    if (event.defaultPrevented) return;
    // Only while this dialog is the topmost layer: a layer opened over it (#1474) is the shell's to close.
    if (event.key === "Escape" && layers.open.at(-1) === layer) {
      event.preventDefault();
      // Rung 1 of the Esc ladder, answered here: the shell must not also unwind what is behind the dialog.
      event.stopPropagation();
      steppedOut = true;
      onclose();
      return;
    }
    if (isSendKey(event) && layers.open.at(-1) === layer) {
      event.preventDefault();
      pressPrimary();
      return;
    }
    if (event.key === "Enter" && event.target instanceof HTMLInputElement && (event.target.type === "radio" || event.target.type === "checkbox")) {
      event.preventDefault();
      if (event.target.type === "radio") pressPrimary();
      return;
    }
    if (event.key !== "Tab") return;
    const items = focusable();
    if (items.length === 0) return;
    const first = items[0]!;
    const last = items[items.length - 1]!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="tf-modal"
  onclickcapture={notePress}
  onkeydowncapture={notePress}
  onmousedown={(event) => {
    if (event.target === event.currentTarget && !leavesOnScrim) event.preventDefault();
  }}
  onclick={(event) => {
    if (event.target === event.currentTarget && leavesOnScrim) onclose();
  }}
>
  <div class="tf-dialog {className}" role="dialog" aria-modal="true" aria-labelledby={titleId} tabindex="-1" bind:this={dialog} {onkeydown}>
    <header class="tf-dhead">
      <div class="tf-dtitle">
        <h2 id={titleId}>{#if typeof title === "string"}<TfKeyed text={title} lead />{:else}{@render title()}{/if}</h2>
        {#if subtitle}<p class="tf-dsub">{subtitle}</p>{/if}
      </div>
      {#snippet close()}
        <button type="button" class="tf-iconbtn tf-dclose" aria-label="Close" onclick={onclose}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      {/snippet}
      {#if actions}
        <div class="tf-dctl">
          <div class="tf-acts">{@render actions()}</div>
          {#if closeButton}<span class="tf-dsep" aria-hidden="true"></span>{@render close()}{/if}
        </div>
      {:else if closeButton}
        {@render close()}
      {/if}
    </header>
    <div class="tf-body">{@render body()}</div>
    {#if footer}<footer>{#if why}<p class="tf-dwhy" role="status">{why}</p>{/if}{@render footer()}</footer>{/if}
  </div>
</div>
