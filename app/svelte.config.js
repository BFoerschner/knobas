import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

// No SvelteKit (global constraint): plain Vite plus the preprocessor that
// makes `<script lang="ts">` work.
export default {
  preprocess: vitePreprocess(),
};
