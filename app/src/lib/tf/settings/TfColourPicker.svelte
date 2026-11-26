<!--
  A swatch's own colour picker (#2516), in its panel over the Hex field: a square of saturation (across) and brightness
  (up) under the hue the strip below it picks. A press or a drag on either picks where the pointer is; the arrow keys
  step it, by 1 (or 10 with Shift) percent on the square and degree on the strip. A drag's moves go to `onpick` with
  `end` false, which the swatch hands on once a frame as a preview, and its release with `end` true, as is every key.
  A grey keeps the hue the strip was on, so brightening it again comes back in that hue.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { fromHsv, toHsv, type Triple } from "../theme/colour";
  import { cssVars } from "../theme/paint";

  let { label, value, onpick }: { label: string; value: string; onpick: (next: string, end: boolean) => void } = $props();

  let hsv = $state<Triple>(untrack(() => toHsv(value)));
  /** The last colour picked here, which coming back as `value` moves nothing. */
  let sent: string | null = null;
  $effect.pre(() => {
    const next = value.toUpperCase();
    untrack(() => {
      if (next === sent) return;
      const [h, s, v] = toHsv(next);
      hsv = [s === 0 || v === 0 ? hsv[0] : h, s, v];
    });
  });

  function send(next: Triple, end: boolean): void {
    hsv = next;
    const hex = fromHsv(next);
    sent = hex;
    onpick(hex, end);
  }

  const clamp = (n: number) => Math.min(1, Math.max(0, n));
  /** Which of the two a pointer holds. */
  let held: "square" | "strip" | null = null;

  function under(event: MouseEvent, which: "square" | "strip"): Triple {
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const x = clamp((event.clientX - box.left) / box.width);
    if (which === "strip") return [x * 360, hsv[1], hsv[2]];
    return [hsv[0], x, 1 - clamp((event.clientY - box.top) / box.height)];
  }

  function down(event: PointerEvent, which: "square" | "strip"): void {
    event.preventDefault();
    const el = event.currentTarget as HTMLElement;
    if (event.pointerId !== undefined) el.setPointerCapture?.(event.pointerId);
    el.focus();
    held = which;
    send(under(event, which), false);
  }
  function move(event: PointerEvent, which: "square" | "strip"): void {
    if (held === which) send(under(event, which), false);
  }
  function up(event: PointerEvent, which: "square" | "strip"): void {
    if (held !== which) return;
    held = null;
    send(event.type === "pointercancel" ? hsv : under(event, which), true);
  }

  function key(event: KeyboardEvent, which: "square" | "strip"): void {
    const [h, s, v] = hsv;
    let next: Triple | null = null;
    if (which === "strip") {
      const by = event.shiftKey ? 10 : 1;
      if (event.key === "ArrowRight" || event.key === "ArrowUp") next = [Math.min(360, h + by), s, v];
      else if (event.key === "ArrowLeft" || event.key === "ArrowDown") next = [Math.max(0, h - by), s, v];
      else if (event.key === "Home") next = [0, s, v];
      else if (event.key === "End") next = [360, s, v];
    } else {
      const by = event.shiftKey ? 0.1 : 0.01;
      if (event.key === "ArrowRight") next = [h, clamp(s + by), v];
      else if (event.key === "ArrowLeft") next = [h, clamp(s - by), v];
      else if (event.key === "ArrowUp") next = [h, s, clamp(v + by)];
      else if (event.key === "ArrowDown") next = [h, s, clamp(v - by)];
    }
    if (next === null) return;
    event.preventDefault();
    send(next, true);
  }

  const percent = (n: number) => Math.round(n * 100);
</script>

<div class="tf-cp">
  <div
    class="tf-cp-sv"
    role="slider"
    tabindex="0"
    aria-label="{label}: saturation and brightness"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={percent(hsv[2])}
    aria-valuetext="saturation {percent(hsv[1])} %, brightness {percent(hsv[2])} %"
    {@attach cssVars(() => ({ "--cp-hue": fromHsv([hsv[0], 1, 1]) }))}
    onpointerdown={(event) => down(event, "square")}
    onpointermove={(event) => move(event, "square")}
    onpointerup={(event) => up(event, "square")}
    onpointercancel={(event) => up(event, "square")}
    onkeydown={(event) => key(event, "square")}
  >
    <i class="tf-cp-at" {@attach cssVars(() => ({ left: `${hsv[1] * 100}%`, top: `${(1 - hsv[2]) * 100}%`, background: fromHsv(hsv) }))}></i>
  </div>
  <div
    class="tf-cp-hue"
    role="slider"
    tabindex="0"
    aria-label="{label}: hue"
    aria-valuemin={0}
    aria-valuemax={360}
    aria-valuenow={Math.round(hsv[0])}
    aria-valuetext="{Math.round(hsv[0])} degrees"
    onpointerdown={(event) => down(event, "strip")}
    onpointermove={(event) => move(event, "strip")}
    onpointerup={(event) => up(event, "strip")}
    onpointercancel={(event) => up(event, "strip")}
    onkeydown={(event) => key(event, "strip")}
  >
    <i class="tf-cp-at" {@attach cssVars(() => ({ left: `${(hsv[0] / 360) * 100}%`, background: fromHsv([hsv[0], 1, 1]) }))}></i>
  </div>
</div>
