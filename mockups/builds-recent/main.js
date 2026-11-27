/* The right side: the scope's header, the one filter bar, the one run table. */
const CHIP = { ok: "tf-ok", err: "tf-err", warn: "tf-warn", run: "tf-info", q: "tf-info", muted: "" };
const RESULTS = ["Failed", "Running", "Queued", "Passed"];
const RESULT = { err: "Failed", run: "Running", q: "Queued", ok: "Passed", warn: "Passed" };
const DOT = { r: "Failed, unread. M marks it read.", b: "Running, not looked at yet. Opening it or M reads it.", g: "Passed while you were away." };
const BAR = 64;

function length(s) {
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
  return `${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}m`;
}
const elapsed = (r) => Math.max(0, (CLOCK.min - (r.started ?? r.at)) * 60 + CLOCK.sec);
const usual = (id) => Math.max(240, ...RUNS.filter((r) => r.pipeline === id && r.secs !== null).map((r) => r.secs));

/** The pipelines the scope holds, and what the header and the crumbs call it. */
function scopeOf() {
  if (S.scope.kind === "pipeline") {
    const p = PIPE.get(S.scope.id);
    return { title: p.name, pipelines: [p], path: p.path, pipeline: p };
  }
  if (S.scope.kind === "fold") return { title: S.scope.path.at(-1), pipelines: PIPELINES.filter((p) => starts(p.path, S.scope.path)), path: S.scope.path, pipeline: null };
  return { title: "All pipelines", pipelines: PIPELINES, path: [], pipeline: null };
}

/** The scope's runs, newest first, and those the bar keeps. */
function streamRuns() {
  const scope = scopeOf();
  const ids = new Set(scope.pipelines.map((p) => p.id));
  let all = S.state === "none" ? [] : RUNS.filter((r) => ids.has(r.pipeline) && !S.waiting.includes(r.id)).sort(newest);
  if (scope.pipeline) all = [...all, ...olderRuns(scope.pipeline, all.at(-1)?.no)];
  const words = S.text.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const text = (r) => { const p = PIPE.get(r.pipeline); return [p.name, ...p.path, r.branch, r.by, r.no === null ? "" : `#${r.no}`].join(" ").toLowerCase(); };
  const kept = all.filter((r) => (!S.mine || r.by === YOU) && (S.results.length === 0 || S.results.includes(RESULT[r.state])) && words.every((w) => text(r).includes(w)));
  return { scope, all, kept };
}
const narrowed = () => S.mine || S.results.length > 0 || S.text.trim() !== "";

function drawHead() {
  const { scope } = streamRuns();
  const crumb = (name, to) => `<a class="tf-crumb" href="#" data-crumb="${esc(to)}">${esc(name)}</a><span class="tf-sep">›</span>`;
  const folders = scope.path.map((g, i) => ({ name: g, to: scope.path.slice(0, i + 1).join("/") }));
  const here = scope.pipeline ? folders : folders.slice(0, -1);
  const crumbs = `<nav class="tf-crumbs" aria-label="Breadcrumb">${S.scope.kind === "all" ? `<span>Builds</span><span class="tf-sep">›</span>` : crumb("Builds", "")}`
    + here.map((f) => crumb(f.name, f.to)).join("") + `<strong>${esc(scope.title)}</strong></nav>`;
  if (!scope.pipeline) {
    document.getElementById("head").innerHTML = crumbs + `<header class="tf-fhead tf-bhead"><div><h1>${esc(scope.title)}</h1></div>
      <div class="tf-acts"><button type="button" class="tf-btn tf-syncnow" title="Run ${SOURCE} now">Sync now</button></div></header>`;
    return;
  }
  const p = scope.pipeline;
  const runs = [...runsOf(p.id), ...olderRuns(p, runsOf(p.id).at(-1)?.no)].slice(0, 12).reverse();
  const most = Math.max(...runs.map((r) => r.secs ?? elapsed(r)), 1);
  const bars = runs.map((r, i) => { const h = 8 + Math.round(28 * (r.secs ?? elapsed(r)) / most); return `<rect class="tf-bar tf-run-${r.state}" x="${i * 14}" y="${40 - h}" width="10" height="${h}" rx="2"/>`; }).join("");
  const done = runs.filter((r) => r.secs !== null);
  document.getElementById("head").innerHTML = crumbs + `<header class="tf-fhead tf-bhead"><div><h1>${esc(p.name)}</h1>
      <div class="tf-sub tf-bsub"><span class="tf-bsub-kind">${SOURCE} build configuration, builds</span><span class="tf-bsub-repo tf-mono">tidewater/${esc(p.path.at(-1).toLowerCase().replace(/\s+/g, "-"))}</span><span class="tf-bsub-sep">.</span><span class="tf-bsub-trig">Runs on every push.</span></div></div>
      <div class="tf-acts"><button type="button" class="tf-btn tf-primary">Run…</button><button type="button" class="tf-btn tf-ghost">${p.pinned ? "Unpin" : "Pin"}</button><button type="button" class="tf-btn tf-ghost">Open in ${SOURCE}</button></div></header>
    <div class="tf-hist"><div class="tf-chart"><svg class="tf-bars" width="${12 * 14}" height="40" aria-hidden="true">${bars}</svg>
      <div class="tf-axis"><span>${OLD_DAYS[1].slice(4)}</span><span>now</span></div></div>
      <div class="tf-facts2"><div><b>Passed</b>${done.filter((r) => r.state === "ok").length} of the last ${done.length}</div><div><b>Usual duration</b>${length(usual(p.id))}</div></div></div>
    <nav class="tf-tabs" aria-label="Pipeline"><a href="#" aria-current="page">Runs</a><a href="#">Parameters</a><a href="#">Triggers</a></nav>`;
}

const countLine = (shown, total) => shown === total ? `${total} ${total === 1 ? "run" : "runs"}` : `${shown} of ${total} runs`;

function drawBar() {
  const { scope, all, kept } = streamRuns();
  const unread = tally(narrowed() ? kept : all);
  const n = unread.b + unread.g + unread.r;
  const what = narrowed() ? "the runs listed" : S.scope.kind === "all" ? "every run of every pipeline" : scope.pipeline ? `every run of ${scope.title}` : `every run in ${scope.path.join(" › ")}`;
  const chips = RESULTS.map((c) => `<button type="button" data-result="${c}" aria-pressed="${S.results.includes(c)}">${c}</button>`).join("");
  document.getElementById("bar").innerHTML = `<div class="tf-fbar">
    <label class="tf-fsearch"><svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><circle cx="7" cy="7" r="5"/><path d="M11 11l3.5 3.5"/></svg>
      <input id="words" placeholder="Filter runs" aria-label="Filter runs" value="${esc(S.text)}"></label>
    <div class="tf-seg" role="group" aria-label="Scope"><button type="button" data-mine="1" aria-pressed="${S.mine}" title="Runs you started">Mine</button><button type="button" data-mine="0" aria-pressed="${!S.mine}">All</button></div>
    <div class="tf-ftail"><div class="tf-fchips" role="group" aria-label="Result">${chips}</div>
      <span class="tf-fend"><span class="tf-fcount" aria-live="polite" data-room="${countLine(Math.max(0, all.length - 1), Math.max(all.length, 10))}"><span class="tf-fcount-n${S.streamRead ? "" : " tf-unseen"}" id="count">${countLine(kept.length, all.length)}</span></span>
        <button type="button" class="tf-btn tf-sm tf-ghost tf-readall" data-readall aria-label="Mark all as read" title="Mark all as read: ${esc(what)}${n > 0 ? `, ${n} unread` : ""}">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M1.5 8.5l3 3 6-7M7.5 11.5l1 0.5 6-7.5"/></svg><span>Mark all as read</span></button></span></div></div>`;
}

/** A name in two parts: its last word, which never goes, and what leads it, which is cut first. */
function nameParts(name) {
  const m = /^(.*[_\s])([^_\s]{3,})$/.exec(name);
  return m && m[1].length >= 3 ? [m[1], m[2]] : ["", name];
}

function runRow(r, scope) {
  const p = PIPE.get(r.pipeline);
  const finished = r.secs !== null;
  const number = r.no === null ? "queued" : `#${r.no}`;
  const dot = r.mark ? `<span class="tf-unread tf-u-${r.mark}" title="${DOT[r.mark]}"><span class="tf-sr">Unread</span></span>` : "";
  const chip = `<span class="tf-chip ${CHIP[r.state]}">${WORD[r.state]}</span>`;
  let lead;
  if (scope.pipeline) lead = `<td class="tf-nowrap tf-run-no">${dot}<a href="#" data-open><b>${number}</b></a>${chip}</td>`;
  else {
    const rel = p.path.slice(scope.path.length);
    const [head, tail] = nameParts(p.name);
    const path = rel.length === 0 ? "<span></span>"
      : `<a class="tf-run-path" href="#" data-crumb="${esc(p.path.join("/"))}" title="Show the runs in ${esc(p.path.join(" › "))}"><bdi>${esc(rel.join(" › "))} ›</bdi></a>`;
    lead = `<td class="tf-nowrap tf-run-no">${dot}${chip}</td><td class="tf-run-pipe"><span class="tf-run-id" title="${esc([...p.path, p.name].join(" › "))}">${path}`
      + `<span class="tf-run-who"><span class="tf-run-nm">${head ? `<span class="tf-run-nmh">${esc(head)}</span>` : ""}<span class="tf-run-nmt">${esc(tail)}</span></span><a class="tf-run-pno" href="#" data-open>${number}</a></span></span></td>`;
  }
  const tests = r.tests === null ? "" : (r.tests[0] > 0 ? `<span class="tf-tests-failed">${r.tests[0]} failed</span>, ` : "") + `${r.tests[1].toLocaleString("en")} passed`;
  const most = usual(r.pipeline);
  const dur = finished ? `<svg class="tf-durbar" width="${BAR}" height="4" aria-hidden="true"><rect width="${Math.max(3, Math.round(BAR * Math.min(1, r.secs / most)))}" height="4" rx="2"/></svg>${length(r.secs)}`
    : r.state === "run" ? `<svg class="tf-durbar tf-live" width="${BAR}" height="4" aria-hidden="true"><rect class="tf-durbar-track" width="${BAR}" height="4" rx="2"/><rect class="tf-durbar-fill" data-fill="${r.id}" width="${Math.round(BAR * Math.min(1, elapsed(r) / most))}" height="4" rx="2"/></svg><span class="tf-info" data-el="${r.id}">${length(elapsed(r))}</span>`
    : `<span class="tf-dim">waiting</span>`;
  const canMark = r.state === "err" || r.state === "run";
  const mark = !canMark ? "" : `<button type="button" class="tf-iconbtn tf-sm" data-mark aria-label="${r.mark ? "Mark as read" : "Mark as unread"}" title="${r.mark ? "Mark as read" : "Mark as unread"} (M)"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${r.mark ? '<path d="M3 8.5l3 3 7-7.5"/>' : '<circle cx="8" cy="8" r="3" fill="currentColor" stroke="none"/>'}</svg></button>`;
  return `<tr class="tf-row${S.arrived.has(r.id) ? " tf-run-new" : ""}" data-run="${r.id}">${lead}`
    + `<td class="tf-mono tf-run-br"><span class="tf-run-brin"><span class="tf-run-brname" title="${esc(r.branch)}">${esc(r.branch)}</span></span></td>`
    + `<td class="tf-dim tf-run-trig" title="${esc(r.by)}">${esc(r.by)}</td><td class="tf-nowrap tf-run-tests">${tests}</td>`
    + `<td class="tf-nowrap tf-num tf-run-dur">${dur}</td><td class="tf-nowrap tf-dim tf-num tf-run-at"><span class="tf-rowval tf-covered">${clock(r.at)}</span></td>`
    + `<td class="tf-nowrap tf-run-acts"><span class="tf-rowend"><span class="tf-rowacts">${mark}<a class="tf-btn tf-sm tf-ghost" href="#">Log</a>${finished ? `<button type="button" class="tf-btn tf-sm tf-ghost">Run again…</button>` : ""}</span></span></td></tr>`;
}

function drawRuns() {
  const box = document.getElementById("runs");
  const { scope, all, kept } = streamRuns();
  const deep = scope.pipelines.some((p) => p.path.length > scope.path.length);
  box.dataset.paths = S.scope.kind === "all" ? "long" : deep ? "short" : "none";
  box.classList.toggle("tf-stream", !scope.pipeline);
  const cols = scope.pipeline ? 7 : 8;
  if (!S.streamRead) {
    /* The table's box and its rows at their height from the first frame, so the runs land in boxes that already stand. */
    const rows = Math.min(40, Math.max(5, Math.ceil((document.querySelector(".tf-bmain").clientHeight - box.offsetTop - 38) / 34) + 2));
    box.innerHTML = `<div class="tf-days"><div class="tf-day tf-day-first"><h3 class="tf-dayh tf-unseen">Today</h3><table class="tf-tbl tf-runtbl tf-readrows" aria-label="Runs"><tbody>`
      + Array.from({ length: rows }, (_, i) => `<tr><td colspan="${cols}" class="tf-dim">${i === 0 ? "Reading runs…" : ""}</td></tr>`).join("") + `</tbody></table></div></div>`;
    return;
  }
  const pill = S.waiting.length > 0 ? `<div class="tf-newruns"><button type="button" data-shownew title="Started since the list drew. They wait here while the pointer is on the runs, so nothing moves under it.">Show ${S.waiting.length} new ${S.waiting.length === 1 ? "run" : "runs"}</button></div>` : "";
  if (kept.length === 0) {
    const words = all.length > 0 ? `<p>No runs match.</p><button type="button" class="tf-btn tf-sm" data-clear>Clear the filter</button>`
      : S.scope.kind === "all" ? `<p>No runs yet. A CI source's runs show here once it has synced.</p><button type="button" class="tf-btn tf-sm">Sync now</button>`
      : `<p>Nothing has run in ${esc(scope.title)} yet.</p>`;
    box.innerHTML = `${pill}<div class="tf-days"><div class="tf-day tf-day-first"><h3 class="tf-dayh tf-unseen">Today</h3><div class="tf-stream-none"><div class="tf-empty">${words}</div></div></div></div>`;
    return;
  }
  const days = [];
  for (const r of kept) {
    const label = r.dayLabel ?? DAYS[r.day];
    if (days.at(-1)?.label !== label) days.push({ label, runs: [] });
    days.at(-1).runs.push(r);
  }
  const end = scope.pipeline ? `<button type="button" class="tf-btn tf-sm tf-ghost">Older runs</button>` : `<p class="tf-dim">Every run knobas holds${S.scope.kind === "all" ? "" : " here"}.</p>`;
  box.innerHTML = pill + `<div class="tf-days">` + days.map((d, i) => `<div class="tf-day${i === 0 ? " tf-day-first" : ""}"><h3 class="tf-dayh">${d.label}</h3>`
    + `<table class="tf-tbl tf-runtbl" aria-label="Runs, ${d.label}"><tbody>${d.runs.map((r) => runRow(r, scope)).join("")}</tbody></table></div>`).join("") + `</div><div class="tf-older">${end}</div>`;
}
