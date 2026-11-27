/* Presses, keys, the live changes and the yellow bar. */
const $ = (id) => document.getElementById(id);
const said = (text) => { $("mk-said").textContent = text; };
const scroller = () => document.querySelector(".tf-bmain");

function draw() { drawTop(); drawRail(); drawHead(); drawBar(); drawRuns(); syncBar(); }

/** The numbers and the dots again, the page where it was and the focused run still focused. */
function redrawMarks() {
  const focused = document.activeElement?.closest?.("tr[data-run]")?.dataset.run;
  drawTop(); drawRail(); drawBar(); drawRuns();
  if (focused) document.querySelector(`tr[data-run="${focused}"] [data-open]`)?.focus({ preventScroll: true });
}

function setScope(scope) {
  S.scope = scope;
  S.arrived.clear();
  if (scope.kind === "fold") S.remembered.set(scope.path.join("/"), true);
  drawRail(); drawHead(); drawBar(); drawRuns(); syncBar();
  scroller().scrollTop = 0;
  document.querySelector('#rail [aria-current="true"]')?.scrollIntoView?.({ block: "nearest" });
}

function applyState(state) {
  S.state = state;
  S.streamRead = S.railRead = state !== "reading";
  S.mine = state === "nomatch";
  S.results = state === "nomatch" ? ["Queued"] : [];
  S.text = "";
}

let toastTimer;
function toast(text, undo) {
  clearTimeout(toastTimer);
  $("toasts").innerHTML = `<div class="tf-toast" role="status"><span class="tf-toast-line"><span class="tf-toast-head">${esc(text)}</span></span>${undo ? '<button type="button" data-undo>Undo</button>' : ""}</div>`;
  toastTimer = setTimeout(() => ($("toasts").innerHTML = ""), 7000);
}

/* *Mark all as read*: every colour of what the stream lists, in one press, with *Undo*. */
function readAll() {
  const { all, kept } = streamRuns();
  const runs = (narrowed() ? kept : all).filter((r) => r.mark);
  if (runs.length === 0) return toast("No unread runs");
  S.undo = runs.map((r) => [r, r.mark]);
  for (const r of runs) r.mark = "";
  toast(`Marked ${runs.length} ${runs.length === 1 ? "run" : "runs"} as read`, true);
  redrawMarks();
}

/* `m`, and the row's own mark: a failure is read or unread, a run in flight is taken out of the blue or put back. */
function toggleMark(id) {
  const r = RUNS.find((x) => x.id === id);
  if (!r || (r.state !== "err" && r.state !== "run")) return;
  r.mark = r.mark ? "" : r.state === "err" ? "r" : "b";
  redrawMarks();
}

function openRun(id) {
  const r = RUNS.find((x) => x.id === id);
  if (!r) return said("This older run is not part of the fixture.");
  const had = r.mark;
  r.mark = "";
  r.looked = true;
  redrawMarks();
  said(`${r.no === null ? "The queued run" : `#${r.no}`} opens on the run's own page, as today (not drawn here). Opening a run reads it${had ? ": its dot went, and so did its number in the rail and the top bar" : ""}.`);
}

/* A change a sync or a refresh brings while the page is open. Every one lands in a box that already stands. */
function step() {
  const e = EVENTS[S.step];
  if (!e) return said("That was the last change. Reload the page to start over.");
  S.step += 1;
  if (e.add) {
    const r = { ...e.add, id: `new${S.step}`, at: CLOCK.min };
    RUNS.push(r);
    const listed = streamRuns().scope.pipelines.some((p) => p.id === r.pipeline);
    if (S.pointerOn && listed) S.waiting.push(r.id); else S.arrived.add(r.id);
    said(`${e.say}. ${S.waiting.length > 0 ? "The pointer rests on the runs, so it waits behind Show 1 new run." : "No pointer rests on the runs, so it took the top, lit once."}`);
  } else {
    const r = RUNS.find(e.run);
    if (!r) return step();
    const to = { ...e.to };
    if (to.secs === "now") to.secs = elapsed(r);
    /* A queued run that starts keeps its row: the list holds its order, and only the chip, the number and the clock change. */
    if (to.state === "run") to.started = CLOCK.min;
    /* A run you opened while it ran ends read: you were watching it. */
    if (r.looked && to.mark) to.mark = "";
    Object.assign(r, to);
    said(`${e.say}.`);
  }
  redrawMarks();
}

function release() {
  for (const id of S.waiting) S.arrived.add(id);
  S.waiting = [];
  drawBar(); drawRuns();
}

function replay() {
  applyState("reading");
  draw();
  said("Nothing read yet: the rail's first row and tools, the header, the bar and the table's box stand from the first frame.");
  setTimeout(() => { S.state = "ready"; S.streamRead = true; drawBar(); drawRuns(); syncBar(); said("The first runs landed in the rows that stood there. Each names its own pipeline and folders, so they need no pipeline list."); }, 1100);
  setTimeout(() => { S.railRead = true; drawRail(); said("The tree and the numbers landed in the rail. Nothing on the right moved."); }, 2400);
}

let playing = null;
function syncBar() {
  const now = { scope: S.scope.kind, state: S.state, away: S.away ? "1" : "0", theme: document.documentElement.dataset.theme };
  for (const b of document.querySelectorAll(".mk button[data-v]")) b.setAttribute("aria-pressed", String(now[b.dataset.mk] === b.dataset.v));
  document.querySelector('.mk [data-mk="play"]').setAttribute("aria-pressed", String(playing !== null));
}

const PRESET = { all: { kind: "all" }, fold: { kind: "fold", path: ["Payments", "Payouts"] }, pipeline: { kind: "pipeline", id: "Payments/Payouts/Payout_IntegrationTests" } };

document.addEventListener("click", (event) => {
  const t = event.target;
  const mk = t.closest(".mk button");
  if (mk) {
    const { mk: what, v } = mk.dataset;
    if (what === "scope") return setScope(PRESET[v]);
    if (what === "state") { applyState(v); return draw(); }
    if (what === "theme") { document.documentElement.dataset.theme = v; return syncBar(); }
    if (what === "away") { S.away = v === "1"; drawTop(); syncBar(); return said(S.away ? "The top bar as Tickets would show it: three numbers, blue, green, red. The page under it is still Builds." : "With Builds in front the green is not said: looking at the page clears it."); }
    if (what === "step") return step();
    if (what === "replay") return replay();
    if (what === "play") { if (playing) { clearInterval(playing); playing = null; } else { step(); playing = setInterval(step, 2600); } return syncBar(); }
    return;
  }
  if (t.closest('a[href="#"]')) event.preventDefault();
  if (t.closest("[data-undo]")) { for (const [r, mark] of S.undo ?? []) r.mark = mark; S.undo = null; $("toasts").innerHTML = ""; return redrawMarks(); }
  if (t.closest("[data-root]")) return setScope({ kind: "all" });
  const tog = t.closest("[data-tog]");
  if (tog) { const row = tog.closest("[data-fold]"); S.remembered.set(row.dataset.fold, row.getAttribute("aria-expanded") !== "true"); return drawRail(); }
  const fold = t.closest("[data-fold]");
  if (fold) return setScope({ kind: "fold", path: fold.dataset.fold.split("/") });
  const pipe = t.closest("[data-pipe]");
  if (pipe) return setScope({ kind: "pipeline", id: pipe.dataset.pipe });
  const crumb = t.closest("[data-crumb]");
  if (crumb) return setScope(crumb.dataset.crumb === "" ? { kind: "all" } : { kind: "fold", path: crumb.dataset.crumb.split("/") });
  const chip = t.closest("[data-chip]");
  if (chip) { const c = chip.dataset.chip; S.chips = S.chips.includes(c) ? S.chips.filter((x) => x !== c) : [...S.chips, c]; return drawRail(); }
  const all = t.closest("[data-all]");
  if (all) { const mark = (f) => { S.remembered.set(f.key, all.dataset.all === "open"); f.folds.forEach(mark); }; treeOf(PIPELINES).forEach(mark); return drawRail(); }
  const result = t.closest("[data-result]");
  if (result) { const c = result.dataset.result; S.results = S.results.includes(c) ? S.results.filter((x) => x !== c) : [...S.results, c]; drawBar(); return drawRuns(); }
  const mine = t.closest("[data-mine]");
  if (mine) { S.mine = mine.dataset.mine === "1"; drawBar(); return drawRuns(); }
  if (t.closest("[data-clear]")) { S.mine = false; S.results = []; S.text = ""; S.state = "ready"; drawBar(); drawRuns(); return syncBar(); }
  if (t.closest("[data-readall]")) return readAll();
  if (t.closest("[data-shownew]")) return release();
  const row = t.closest("tr[data-run]");
  if (t.closest("[data-mark]")) return toggleMark(row.dataset.run);
  if (row && (t.closest("[data-open]") || !t.closest("a, button"))) return openRun(row.dataset.run);
});

document.addEventListener("input", (event) => {
  if (event.target.id !== "words") return;
  S.text = event.target.value;
  drawRuns();
  const { all, kept } = streamRuns();
  $("count").textContent = countLine(kept.length, all.length);
});

/* The rail's keys are the tree's, as today; Enter on a folder selects it. The runs' are Up, Down, Enter and M. */
document.addEventListener("keydown", (event) => {
  if (event.altKey || event.ctrlKey || event.metaKey || event.target.matches("input")) return;
  const run = event.target.closest?.("tr[data-run]");
  if (run) {
    if (event.key === "m") { event.preventDefault(); return toggleMark(run.dataset.run); }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const links = [...document.querySelectorAll("tr[data-run] [data-open]")];
    const to = links[links.findIndex((l) => run.contains(l)) + (event.key === "ArrowDown" ? 1 : -1)];
    event.preventDefault();
    to?.focus();
    return to?.scrollIntoView({ block: "nearest" });
  }
  const row = event.target.closest?.("#rail .tf-tr");
  if (!row) return;
  const rows = [...document.querySelectorAll("#rail .tf-tr")];
  const at = rows.indexOf(row);
  const key = row.dataset.key;
  const open = row.getAttribute("aria-expanded");
  let next = null;
  if (event.key === "ArrowDown") next = rows[at + 1];
  else if (event.key === "ArrowUp") next = rows[at - 1];
  else if (event.key === "Home") next = rows[0];
  else if (event.key === "End") next = rows.at(-1);
  else if (event.key === "Enter" && row.dataset.fold) { event.preventDefault(); setScope({ kind: "fold", path: row.dataset.fold.split("/") }); }
  else if ((event.key === "ArrowRight" && open === "false") || (event.key === "ArrowLeft" && open === "true")) { S.remembered.set(row.dataset.fold, open === "false"); drawRail(); }
  else return;
  event.preventDefault();
  (next ?? document.querySelector(`#rail [data-key="${CSS.escape(key)}"]`))?.focus();
});

$("runs").addEventListener("pointerenter", () => (S.pointerOn = true));
$("runs").addEventListener("pointerleave", () => { S.pointerOn = false; if (S.waiting.length > 0) release(); });

/* A run in flight counts up where it stands: the words and the bar's fill, no other box. */
setInterval(() => {
  CLOCK.sec += 1;
  if (CLOCK.sec === 60) { CLOCK.sec = 0; CLOCK.min += 1; }
  for (const el of document.querySelectorAll("[data-el]")) { const r = RUNS.find((x) => x.id === el.dataset.el); if (r) el.textContent = length(elapsed(r)); }
  for (const el of document.querySelectorAll("[data-fill]")) { const r = RUNS.find((x) => x.id === el.dataset.fill); if (r) el.setAttribute("width", String(Math.round(BAR * Math.min(1, elapsed(r) / usual(r.pipeline))))); }
}, 1000);

document.documentElement.dataset.theme = params.get("theme") ?? (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
document.documentElement.dataset.bar = params.get("bar") ?? "1";
applyState(S.state);
S.scope = PRESET[params.get("scope")] ?? PRESET.all;
if (S.scope.kind === "fold") S.remembered.set(S.scope.path.join("/"), true);
draw();
/* ?steps=N plays the first N live changes at once, for a still of the page after them. */
for (let i = 0; i < Number(params.get("steps") ?? 0); i += 1) step();
