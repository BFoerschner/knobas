// The Code area mockup (#2252): shared chrome, theme, states and the few interactions a reviewer can click.
// Address: <page>.html?theme=light|dark&state=<state>&layout=unified|split&bare (bare hides the mockup bar for shots).
(function () {
  const q = new URLSearchParams(location.search);
  const root = document.documentElement;
  let theme = q.get("theme");
  try { theme = theme || localStorage.getItem("knobas-mockup-theme"); } catch (e) { /* storage off */ }
  if (theme === "dark" || theme === "light") root.dataset.theme = theme;

  const ICONS = {
    folder: '<path d="M2.5 4.5h4l1.5 1.5h5.5v6.5h-11z"/>',
    "folder-open": '<path d="M2.5 4.5h4l1.5 1.5h5.5v1.5M2.5 4.5v8h10l1.5-5h-10l-1.5 5"/>',
    file: '<path d="M4 2h5l3 3v9H4z M9 2v3h3"/>',
    "chev-r": '<path d="M6 4l4 4-4 4"/>', "chev-d": '<path d="M4 6l4 4 4-4"/>', "chev-u": '<path d="M4 10l4-4 4 4"/>',
    search: '<circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5l3 3"/>',
    ext: '<path d="M9 2.5h4.5V7M13.5 2.5L7.5 8.5M12 9.5v4H2.5V4h4"/>',
    refresh: '<path d="M13 4v3.5H9.5M3 12V8.5h3.5M12.6 7.5A5 5 0 0 0 3.8 5.5M3.4 8.5a5 5 0 0 0 8.8 2"/>',
    plus: '<path d="M8 3v10M3 8h10"/>', x: '<path d="M4 4l8 8M12 4l-8 8"/>',
    grip: '<circle cx="6" cy="4" r=".6"/><circle cx="10" cy="4" r=".6"/><circle cx="6" cy="8" r=".6"/><circle cx="10" cy="8" r=".6"/><circle cx="6" cy="12" r=".6"/><circle cx="10" cy="12" r=".6"/>',
    check: '<path d="M3 8.5l3 3 7-7"/>', more: '<circle cx="3.5" cy="8" r=".8"/><circle cx="8" cy="8" r=".8"/><circle cx="12.5" cy="8" r=".8"/>',
    branch: '<circle cx="4.5" cy="3.5" r="1.5"/><circle cx="4.5" cy="12.5" r="1.5"/><circle cx="11.5" cy="5" r="1.5"/><path d="M4.5 5v6M11.5 6.5c0 3-7 2-7 4.5"/>',
    commit: '<circle cx="8" cy="8" r="2.5"/><path d="M1.5 8h4M10.5 8h4"/>',
    collapse: '<path d="M4 6l4-3 4 3M4 10l4 3 4-3"/>', copy: '<rect x="5" y="5" width="8.5" height="8.5" rx="1"/><path d="M3 10.5V2.5h8"/>',
    eye: '<path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/>',
    sun: '<circle cx="8" cy="8" r="3"/><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.4 1.4M11.6 11.6 13 13M3 13l1.4-1.4M11.6 4.4 13 3"/>',
    gear: '<circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/>',
  };
  window.ic = (name, cls) => `<svg class="i${cls ? " " + cls : ""}" viewBox="0 0 16 16" aria-hidden="true">${ICONS[name] || ""}</svg>`;

  function topbar() {
    const nav = [["Today", "1"], ["Tickets", "2"], ["Code", "3"], ["Docs", "4"], ["Builds", "5"], ["Time", "6"]];
    return `<header class="tf-top"><span class="tf-brand">knobas</span><nav class="tf-nav">${nav.map(([n, k]) =>
      `<a href="#" ${n === "Code" ? 'aria-current="page"' : ""}>${n} <span class="tf-navkey">${k}</span></a>`).join("")}</nav>
      <span class="spacer"></span><span class="tf-search">${ic("search")} Search <kbd>⌘K</kbd></span>
      <button class="tf-iconbtn" title="Theme" id="themebtn">${ic("sun")}</button><button class="tf-iconbtn" title="Settings">${ic("gear")}</button></header>`;
  }
  function crumbs(last) {
    return `<nav class="tf-crumbs"><a class="tf-jobchip" href="#">PAY-214</a><span class="sep">›</span><a href="#">Code</a><span class="sep">›</span>
      ${last}<span class="esc">Back to PAY-214 <kbd>Esc</kbd></span></nav>`;
  }
  const openin = (size) => `<button class="tf-btn tf-ghost${size ? " " + size : ""}">${ic("ext")} Open in ${ic("chev-d", "s")}</button>`;
  window.openin = openin;

  // The repository page's header and its regrouped tabs: the repository at a ref | work in flight.
  function repoHead(tab) {
    const t = (id, label, count) => `<a href="${{ repo: "repo.html", prs: "pr.html" }[id] || "#"}" ${tab === id ? 'aria-current="page"' : ""}>${label}${count ? ` <span class="tf-count">${count}</span>` : ""}</a>`;
    return crumbs("<strong>acme/payments-api</strong>") + `
      <header class="tf-fhead"><div><h1 class="mono">acme/payments-api</h1>
        <div class="sub">Gitea · default branch <span class="mono">main</span> · pushed 2 h ago · cloned at <span class="mono">~/src/payments-api</span>, clean</div></div>
        <div class="tf-acts"><a class="tf-btn tf-primary" href="#">New pull request</a>
          <span class="tf-select">${ic("branch")}<b class="mono">main</b>${ic("chev-d", "s")}</span>
          <button class="tf-btn">New branch…</button>${openin()}<button class="tf-btn">${ic("refresh")} Refresh</button></div></header>
      <nav class="tf-tabs" aria-label="Repository">${t("repo", "Files")}${t("commits", "Commits", "")}${t("branches", "Branches", "14")}${t("compare", "Compare")}
        <span class="grp" aria-hidden="true"></span>${t("prs", "Pull requests", "3")}${t("local", "Local changes", "")}</nav>`;
  }
  function prHead(tab) {
    const t = (id, href, label, count) => `<a href="${href}" ${tab === id ? 'aria-current="page"' : ""}>${label}${count ? ` <span class="tf-count">${count}</span>` : ""}</a>`;
    const ready = q.get("state") === "ready";
    return crumbs('<a href="repo.html">acme/payments-api</a><span class="sep">›</span><strong>#412</strong>') + `
      <header class="tf-fhead"><div><h1>#412 Retry webhook deliveries with backoff</h1>
        <div class="sub"><span class="tf-chip ok">Open</span><span class="mono">feat/webhook-retry</span> into <span class="mono">main</span>, by mara · 6 commits · head <span class="mono">9f3e1c2</span></div></div>
        <div class="tf-acts"><button class="tf-btn tf-primary" ${ready ? "" : 'aria-disabled="true" title="Not ready: see the side rail"'}>Merge…</button>
          <button class="tf-btn tf-warnbtn">Submit review (2 pending)</button>${openin()}<button class="tf-btn">${ic("refresh")} Refresh</button></div></header>
      <nav class="tf-tabs" aria-label="Pull request">${t("conv", "pr.html", "Conversation", "5")}${t("diff", "diff.html", "Files changed", "11")}${t("commits", "#", "Commits", "6")}</nav>`;
  }

  // The mockup bar: not part of the app. Surfaces, the page's states, theme.
  const PAGES = [["repo", "Repository"], ["file", "Renderer switcher"], ["pipeline", "Pipeline editor"], ["pr", "Pull request"], ["diff", "Diff"]];
  function mockbar(page, states) {
    const cur = q.get("state") || states[0];
    const link = (over) => { const p = new URLSearchParams(location.search); for (const k in over) p.set(k, over[k]); return "?" + p.toString(); };
    return `<div class="mockbar"><b>Mockup #2252</b><a href="index.html">Index</a>${PAGES.map(([id, n]) => `<a href="${id}.html${theme ? "?theme=" + theme : ""}" ${id === page ? 'class="on"' : ""}>${n}</a>`).join("")}
      ${states.length > 1 ? `<span class="sepv"></span>State ${states.map((s) => `<a href="${link({ state: s })}" ${s === cur ? 'class="on"' : ""}>${s}</a>`).join("")}` : ""}
      <span class="sepv"></span><a href="${link({ theme: root.dataset.theme === "dark" ? "light" : "dark" })}">${root.dataset.theme === "dark" ? "Light" : "Dark"}</a></div>`;
  }

  document.addEventListener("DOMContentLoaded", () => {
    const b = document.body;
    const page = b.dataset.page, states = (b.dataset.states || "default").split(" ");
    const state = q.get("state") || states[0];
    root.dataset.state = state;
    document.querySelectorAll("[data-when]").forEach((el) => { if (!el.dataset.when.split(" ").includes(state)) el.remove(); });
    document.querySelectorAll("[data-not]").forEach((el) => { if (el.dataset.not.split(" ").includes(state)) el.remove(); });
    if (state === "refresh") document.querySelectorAll("[data-dim]").forEach((el) => el.classList.add("dimmed"));
    document.querySelectorAll("[data-ic]").forEach((el) => el.insertAdjacentHTML("afterbegin", ic(el.dataset.ic, el.dataset.icc)));
    const slot = document.getElementById("chrome");
    if (slot) slot.outerHTML = topbar() + (b.dataset.kind === "pr" ? prHead(b.dataset.tab) : repoHead(b.dataset.tab));
    document.querySelectorAll("[data-openin]").forEach((el) => (el.outerHTML = openin(el.dataset.openin)));
    if (!q.has("bare")) b.insertAdjacentHTML("beforeend", mockbar(page, states));
    // ?check: layout self-test for the implementer (page overflow, a control row that wraps, mixed heights in one row).
    if (q.has("check")) setTimeout(() => {
      const out = [];
      const fams = new Set([...document.fonts].filter((f) => f.status === "loaded").map((f) => f.family.replace(/"/g, "")));
      if (!fams.has("Archivo") || !fams.has("JetBrains Mono")) out.push("fonts not loaded: " + [...fams].join(","));
      if (document.documentElement.scrollWidth > innerWidth) out.push("page overflows: " + document.documentElement.scrollWidth);
      document.querySelectorAll(".tf-acts, .dtool, .pipe .steps, .df > header, .fileview > header").forEach((row, i) => {
        const kids = [...row.children].filter((k) => k.offsetParent !== null && getComputedStyle(k).position !== "absolute");
        const tops = new Set(kids.map((k) => Math.round(k.getBoundingClientRect().top + k.getBoundingClientRect().height / 2)));
        if (tops.size > 1) out.push("row not on one line: " + row.className + " #" + i + " " + [...tops].join(","));
        const hs = new Set(kids.filter((k) => k.matches(".tf-btn, .tf-select, .tf-seg, .tf-pill, .tf-iconbtn, .viewed")).map((k) => Math.round(k.getBoundingClientRect().height)));
        if (hs.size > 1) out.push("mixed control heights: " + row.className + " #" + i + " " + [...hs].join(","));
      });
      document.querySelectorAll(".tf-menu").forEach((m) => { const r = m.getBoundingClientRect(); if (r.right > innerWidth || r.left < 0) out.push("menu off screen: " + Math.round(r.left) + ".." + Math.round(r.right)); });
      document.body.insertAdjacentHTML("beforeend", `<pre id="check">${out.length ? out.join("\n") : "CHECK OK"}</pre>`);
    }, 300);
    const tb = document.getElementById("themebtn");
    if (tb) tb.onclick = () => {
      const next = root.dataset.theme === "dark" ? "light" : "dark";
      try { localStorage.setItem("knobas-mockup-theme", next); } catch (e) { /* storage off */ }
      const p = new URLSearchParams(location.search); p.set("theme", next); location.search = p.toString();
    };
    // Collapse a diff file by its caret; hover a diff line to show its + comment button.
    document.querySelectorAll(".df > header .caret").forEach((c) => (c.onclick = () => c.closest(".df").classList.toggle("closed")));
    document.querySelectorAll(".dl .ln").forEach((l) => { l.onmouseenter = () => l.classList.add("hov"); l.onmouseleave = () => l.classList.remove("hov"); });
  });
})();
