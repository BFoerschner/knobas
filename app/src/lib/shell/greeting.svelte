<!--
  A throwaway component the harness proves itself on. It exists so that
  "mounting a Svelte 5 component under jsdom works, and a rune change lands in
  the DOM" is a fact rather than an assumption; task 5 deletes it, when there
  are real components to make the same point.
-->
<script lang="ts">
  let { name }: { name: string } = $props();
</script>

hello {name}
