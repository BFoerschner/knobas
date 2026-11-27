/* The state, the rules both panes share, the rail and the top bar's numbers. */
const params = new URLSearchParams(location.search);
const S = {
  /* What the rail has selected, which is what the stream lists: { kind: "all" } | { kind: "fold", path } | { kind: "pipeline", id }. */
  scope: { kind: "all" },
  state: params.get("state") ?? "ready",
  streamRead: true, railRead: true,
  chips: [], results: [], mine: false, text: "",
  remembered: new Map(),
  away: params.get("away") === "1",
  waiting: [], arrived: new Set(), pointerOn: false, undo: null, step: 0,
};

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const newest = (a, b) => a.day - b.day || b.at - a.at;
const starts = (path, head) => head.every((g, i) => path[i] === g);
const clock = (min) => `${String(Math.floor(min / 60)).padStart(2, "0")}:${String(min % 60).padStart(2, "0")}`;
const runsOf = (id) => RUNS.filter((r) => r.pipeline === id).sort(newest);
const latestOf = (p) => runsOf(p.id)[0];
const isFailing = (p) => latestOf(p)?.state === "err";
const isRunning = (p) => runsOf(p.id).some((r) => r.state === "run");
/** What a list of runs adds to the three bubbles. */
function tally(runs) {
  const t = { b: 0, g: 0, r: 0 };
  for (const r of runs) if (r.mark) t[r.mark] += 1;
  return t;
}
const tallyOf = (pipelines) => tally(RUNS.filter((r) => pipelines.some((p) => p.id === r.pipeline)));

/* TfRunGlyph.svelte, as it is. */
const GLYPH = {
  ok: '<path class="tf-rg-line" d="M3.5 8.5 6.5 11.5 12.5 4.5" stroke-width="2"/>',
  err: '<circle cx="8" cy="8" r="7" fill="currentColor"/><path class="tf-rg-x" d="M5.4 5.4l5.2 5.2M10.6 5.4l-5.2 5.2" stroke-width="1.8"/>',
  warn: '<path d="M8 1.5 15 14H1z" fill="currentColor"/><path class="tf-rg-x" d="M8 6v3.6" stroke-width="1.7"/><circle class="tf-rg-dot" cx="8" cy="11.8" r=".95"/>',
  run: '<circle class="tf-rg-pulse" cx="8" cy="8" r="5" fill="currentColor"/>',
  q: '<circle class="tf-rg-dash" cx="8" cy="8" r="5.6" stroke-width="1.6" stroke-dasharray="2.4 2.2"/>',
  muted: '<circle class="tf-rg-line" cx="8" cy="8" r="5.6" stroke-width="1.6"/><path class="tf-rg-line" d="M4.2 11.8 11.8 4.2" stroke-width="1.6"/>',
  none: '<path class="tf-rg-line" d="M4.5 8h7" stroke-width="1.6"/>',
};
const WORD = { ok: "Passed", err: "Failed", warn: "Warnings", run: "Running", q: "Queued", muted: "Cancelled", none: "No runs" };
const glyph = (tone) => `<svg class="tf-rg tf-rg-${tone}" viewBox="0 0 16 16" width="12" height="12" role="img" aria-label="${WORD[tone]}">${GLYPH[tone]}</svg>`;

/* The three columns. Always three cells, so each colour has one place; an empty cell keeps its room. */
const SAID = { b: "running, not looked at", g: "passed while you were away", r: "failed, unread" };
function bubbles(t, seen = true) {
  const cell = (k) => `<i class="tf-ub-${k}"${t[k] > 0 ? ` title="${t[k]} ${SAID[k]}"` : ""}>${t[k] > 0 ? Math.min(t[k], 99) : ""}</i>`;
  return `<span class="tf-ub${seen ? "" : " tf-unseen"}">${cell("b")}${cell("g")}${cell("r")}</span>`;
}

/* builds.ts's sharedPrefix: what a fold's own pipelines share, cut at a separator. */
function sharedPrefix(names) {
  if (names.length < 2) return "";
  let common = names[0];
  for (const n of names) while (!n.startsWith(common)) common = common.slice(0, -1);
  const cut = /^.*[_.\-\s:/]/.exec(common)?.[0] ?? "";
  return names.every((n) => n.length > cut.length) ? cut : "";
}

/* treeOf + fold(), as builds.ts has them, each fold with its path and what its runs add to the bubbles. */
function treeOf(list) {
  const root = { name: "", pipelines: [], kids: new Map() };
  for (const p of list) {
    let at = root;
    for (const g of p.path) {
      if (!at.kids.has(g)) at.kids.set(g, { name: g, pipelines: [], kids: new Map() });
      at = at.kids.get(g);
    }
    at.pipelines.push(p);
  }
  const fold = (node, above) => {
    let name = node.name, at = node;
    const path = [...above, node.name];
    while (at.pipelines.length === 0 && at.kids.size === 1) { at = [...at.kids.values()][0]; name += ` › ${at.name}`; path.push(at.name); }
    const folds = [...at.kids.values()].sort((a, b) => a.name.localeCompare(b.name)).map((k) => fold(k, path));
    const under = [...at.pipelines, ...folds.flatMap((f) => f.under)];
    return { name, key: path.join("/"), path, from: above.length + 1, pipelines: at.pipelines, folds, under, failing: under.filter(isFailing).length };
  };
  return [...root.kids.values()].sort((a, b) => a.name.localeCompare(b.name)).map((k) => fold(k, []));
}
const keptPipelines = () => PIPELINES.filter((p) => (!S.chips.includes("Failing") || isFailing(p)) && (!S.chips.includes("Running") || isRunning(p)));
const foldHolds = (f, id) => f.under.some((p) => p.id === id);
/** Whether the fold is the scope: its path, or a chain of single folders that ends past it. */
const foldIsScope = (f) => S.scope.kind === "fold" && S.scope.path.length >= f.from && S.scope.path.length <= f.path.length && starts(f.path, S.scope.path);

function railRows() {
  const list = keptPipelines();
  const rows = [];
  const narrowed = S.chips.length > 0;
  const at = S.scope.kind === "pipeline" ? S.scope.id : null;
  const asks = (f) => f.under.some((p) => latestOf(p)?.mark === "r");
  /* As today: open on what narrows, on a failure that asks, on the pipeline you are on. New: on the folder you are on. */
  const open = (f) => S.remembered.get(f.key) ?? (narrowed || asks(f) || foldHolds(f, at) || (S.scope.kind === "fold" && (starts(S.scope.path, f.path) || foldIsScope(f))));
  const pinned = list.filter((p) => p.pinned);
  if (pinned.length > 0) rows.push({ kind: "head", text: "Pinned" }, ...pinned.map((p) => ({ kind: "pipeline", key: `pin:${p.id}`, p, depth: 0, pre: "" })));
  const walk = (pipelines, folds, depth) => {
    const pre = sharedPrefix(pipelines.map((p) => p.name));
    for (const p of pipelines) rows.push({ kind: "pipeline", key: `tree:${p.id}`, p, depth, pre });
    for (const f of folds) {
      const isOpen = open(f);
      rows.push({ kind: "fold", key: `fold:${f.key}`, f, depth, open: isOpen });
      if (isOpen) walk(f.pipelines, f.folds, depth + 1);
    }
  };
  if (list.length > 0) rows.push({ kind: "head", text: SOURCE });
  walk([], treeOf(list), 0);
  return { rows, list };
}

function railPipeline({ p, depth, pre, key }) {
  const latest = latestOf(p);
  const tone = latest?.state ?? "none";
  const asking = latest?.mark === "r";
  const when = tone === "q" ? `<span class="tf-pi-when">queued</span>`
    : tone === "run" ? `<span class="tf-pi-when tf-info">${clock(latest.at)}</span>`
    : tone === "err" ? `<span class="tf-pi-when${asking ? " tf-err" : ""}">${latest.day === 0 ? clock(latest.at) : DAYS[latest.day].toLowerCase()}</span>` : "";
  const current = S.scope.kind === "pipeline" && S.scope.id === p.id;
  return `<a class="tf-tr tf-pi tf-d${depth}" href="#" data-pipe="${esc(p.id)}" data-key="${esc(key)}" aria-current="${current}" title="${esc([...p.path, p.name].join(" › "))}\n${WORD[tone]}">`
    + glyph(tone)
    + `<span class="tf-pi-nm${asking ? " tf-bad" : ""}">${pre ? `<span class="tf-pi-pre">${esc(pre)}</span>` : ""}<span class="tf-pi-rest">${esc(p.name.slice(pre.length))}</span></span>`
    + when + bubbles(tallyOf([p])) + `</a>`;
}

function railFold({ f, depth, open, key }) {
  return `<div class="tf-tr tf-tfold tf-pi tf-d${depth}" role="treeitem" tabindex="0" data-fold="${esc(f.key)}" data-key="${esc(key)}" aria-expanded="${open}" aria-current="${foldIsScope(f)}"`
    + ` title="${esc(f.path.join(" › "))}\n${f.under.length} pipelines${f.failing > 0 ? `, ${f.failing} failing` : ""}">`
    + `<span class="tf-fold-tog" data-tog title="${open ? "Fold" : "Unfold"}"></span><span class="tf-fold-nm">${esc(f.name)}</span><span class="tf-fold-n">${f.under.length}</span>`
    + bubbles(tallyOf(f.under)) + `</div>`;
}

function drawRail() {
  const none = S.state === "none";
  const { rows, list } = none ? { rows: [], list: [] } : railRows();
  const read = S.railRead && !none;
  const body = !S.railRead ? "" : rows.map((r) => r.kind === "head" ? `<h3>${esc(r.text)}</h3>` : r.kind === "pipeline" ? railPipeline(r) : railFold(r)).join("");
  const chips = ["Failing", "Running"].map((c) => `<button type="button" data-chip="${c}" aria-pressed="${S.chips.includes(c)}">${c}</button>`).join("");
  const anyOpen = rows.some((r) => r.kind === "fold" && r.open);
  const note = none ? `<p class="tf-pl-note">No pipelines yet. A CI source's pipelines show here once it has synced.</p>`
    : S.railRead && list.length === 0 ? `<p class="tf-pl-note">No pipeline matches.</p>` : "";
  document.getElementById("rail").innerHTML = `
    <div class="tf-pl-home"><a class="tf-tr tf-pi tf-d0 tf-pl-root" href="#" data-root data-key="root" aria-current="${S.scope.kind === "all"}" title="Every pipeline's runs, newest first">
      <svg class="tf-pl-rooticon" viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4h12M2 8h12M2 12h8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>
      <span class="tf-pi-nm">All pipelines</span><span class="tf-fold-n${read ? "" : " tf-unseen"}">${PIPELINES.length}</span>${bubbles(tally(none ? [] : RUNS), read)}</a></div>
    <div class="tf-pl-top">
      <div class="tf-pl-group"><div class="tf-seg" role="group" aria-label="Group by"><button type="button" aria-pressed="true">By project</button><button type="button" aria-pressed="false" aria-disabled="true" title="As it is today. Not drawn in this mockup.">By repository</button></div></div>
      <div class="tf-pl-tools"><div class="tf-fchips" role="group" aria-label="Show only">${chips}</div>
        <div class="tf-trail">${rows.some((r) => r.kind === "fold") ? `<button type="button" class="tf-btn tf-sm tf-ghost" data-all="${anyOpen ? "close" : "open"}">${anyOpen ? "Collapse all" : "Expand all"}</button>` : ""}</div></div>
    </div>
    <div class="tf-pl-list">${note}<div class="tf-pl-rows" role="tree">${body}</div></div>`;
}

/* The top bar's Builds entry: blue, green, red, each hidden at zero. With Builds in front the green is not said:
   looking at the page is what clears it. */
function drawTop() {
  const t = tally(S.state === "none" ? [] : RUNS);
  const one = (k, tone, n) => n > 0 ? `<span class="tf-count tf-${tone}" title="${n} ${SAID[k]}">${n}</span>` : "";
  const all = one("b", "info", t.b) + (S.away ? one("g", "ok", t.g) : "") + one("r", "err", t.r);
  document.getElementById("nav-builds").innerHTML = `Builds${all ? ` <span class="tf-nav-ub">${all}</span>` : ""}`;
  for (const a of document.querySelectorAll(".tf-nav a")) a.removeAttribute("aria-current");
  document.getElementById(S.away ? "nav-tickets" : "nav-builds").setAttribute("aria-current", "page");
}
