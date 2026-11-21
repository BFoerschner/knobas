<!--
  *Ignore whitespace* (#2253, #2319), in the diff's toolbar: on offer where the diff was read at a checkout, and
  disabled with its reason as a title where the source served it.
-->
<script lang="ts">
  let {
    checked,
    why = null,
    onchange,
    focus = false,
    onleave = () => {},
  }: {
    checked: boolean;
    /** Why it can't be ticked here; `null` where it can. */
    why?: string | null;
    onchange: (on: boolean) => void;
    /** Takes focus as it is drawn: a press that redraws the diff around it keeps the reader's place. */
    focus?: boolean;
    /** Focus moved on to something else. */
    onleave?: () => void;
  } = $props();
</script>

<label class="tf-ck tf-wsbox" title={why ?? undefined}>
  <input
    type="checkbox"
    {checked}
    disabled={why !== null}
    title={why ?? undefined}
    onchange={(event) => onchange(event.currentTarget.checked)}
    onblur={(event) => {
      if (event.relatedTarget !== null) onleave();
    }}
    {@attach (node) => {
      if (focus) node.focus();
    }}
  />
  Ignore whitespace
</label>
