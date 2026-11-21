// The window's side and theme before the first paint (ADR-0055). A classic script in each document's <head>, so the
// parser runs it before anything is drawn: a module (main.ts) runs after the first paint. It writes what
// `tf/theme.svelte.ts` and `tf/themes.svelte.ts` last wrote, from `localStorage`, and derives nothing; the modules
// write the same values again, so a missing or stale cache costs one frame, never a theme that stays wrong.
(function () {
  try {
    var html = document.documentElement;
    var store = window.localStorage;
    var mode = store.getItem("knobas.theme.mode");
    if (mode === "light" || mode === "dark") html.setAttribute("data-theme", mode);
    // `knobas --default-theme`: this run draws knobas's own themes.
    if (window.__knobasDefaultTheme === true) return;
    var paint = JSON.parse(store.getItem("knobas.paint") || "null");
    if (!paint || paint.v !== 1 || typeof paint.vars !== "object" || paint.vars === null) return;
    for (var name in paint.vars) {
      var value = paint.vars[name];
      if (/^--[a-z0-9-]+-(light|dark)$/.test(name) && typeof value === "string") html.style.setProperty(name, value);
    }
    if (typeof paint.sel === "string" && /^(light|dark)( (light|dark))?$/.test(paint.sel)) html.setAttribute("data-sel", paint.sel);
  } catch (e) {
    // No storage, or a cache that is not ours: the modules draw the theme.
  }
})();
