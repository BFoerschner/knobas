// The frame probes' trace readers (testenv/perf/README.md). What the compositor says reached the screen, from a trace of the `disabled-by-default-devtools.timeline.frame`
// category: one PipelineReporter per display refresh the renderer was asked to draw, with how it ended.
//
//   STATE_PRESENTED_ALL       the frame was shown with everything in it, the main thread's update included
//   STATE_PRESENTED_PARTIAL   a frame was shown without the main thread's update (it missed the deadline)
//   STATE_DROPPED             nothing was shown for that refresh
//   STATE_NO_UPDATE_DESIRED   nothing had changed, so nothing was owed
//
// `scroll_state` says which thread moves the content of the scroll in progress. With SCROLL_MAIN_THREAD (a scroller
// the engine did not give a layer of its own) the list is somewhere new on screen only in a PRESENTED_ALL frame:
// a partial frame shows the old position. With SCROLL_COMPOSITOR_THREAD every presented frame moves it.
//
// The end of a presented reporter is the frame's presentation time as the GPU process reported it, so gaps between
// those ends are gaps between pictures on the display.
const q = (sorted, p) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))] : 0);
const r1 = (x) => Math.round(x * 10) / 10;
const dist = (arr) => { const s = [...arr].sort((a, b) => a - b); return { n: s.length, p50: r1(q(s, 0.5)), p90: r1(q(s, 0.9)), p99: r1(q(s, 0.99)), max: r1(s[s.length - 1] ?? 0) }; };

const STAGES = ["BeginImplFrameToSendBeginMainFrame", "SendBeginMainFrameToCommit", "Commit", "EndCommitToActivation", "Activation", "EndActivateToSubmitCompositorFrame", "SubmitCompositorFrameToPresentationCompositorFrame"];

/**
 * `events`: the trace. `intervalMs`: the display's interval. The window analysed is the span in which reporters
 * carry a scroll (scroll_state other than SCROLL_NONE), or [fromUs, toUs] when given.
 */
export function analyse(events, intervalMs, { fromUs = null, toUs = null, commitsOnly = false } = {}) {
  const open = new Map();
  const frames = [];
  const stageOpen = new Map();
  for (const e of events) {
    if (!e.cat || !e.cat.includes("devtools.timeline.frame") || !e.id2) continue;
    const key = `${e.pid}:${e.id2.local ?? e.id2.global}:${e.name}`;
    if (e.name === "PipelineReporter") {
      if (e.ph === "b") open.set(key, { begin: e.ts, info: e.args?.frame_reporter ?? {}, stages: {}, id: `${e.pid}:${e.id2.local ?? e.id2.global}` });
      else if (e.ph === "e") { const f = open.get(key); if (f) { open.delete(key); f.end = e.ts; frames.push(f); } }
    } else if (STAGES.includes(e.name)) {
      if (e.ph === "b") stageOpen.set(key, e.ts);
      else if (e.ph === "e" && stageOpen.has(key)) {
        const begin = stageOpen.get(key); stageOpen.delete(key);
        const parent = open.get(`${e.pid}:${e.id2.local ?? e.id2.global}:PipelineReporter`);
        if (parent) parent.stages[e.name] = (e.ts - begin) / 1000;
      }
    }
  }
  frames.sort((a, b) => a.begin - b.begin);
  // commitsOnly: a scroll driven from script has no input scroll to mark its frames, and what moves the list is the
  // main thread's commit; the window is then from the first frame that carried a commit to the last.
  const scrolling = commitsOnly ? frames.filter((f) => f.stages.Commit !== undefined) : frames.filter((f) => f.info.scroll_state && f.info.scroll_state !== "SCROLL_NONE");
  if (fromUs === null) fromUs = scrolling.length ? scrolling[0].begin : (frames[0]?.begin ?? 0);
  if (toUs === null) toUs = scrolling.length ? scrolling[scrolling.length - 1].begin : (frames[frames.length - 1]?.begin ?? 0);
  const inWin = frames.filter((f) => f.begin >= fromUs && f.begin <= toUs && f.info.frame_type !== "FORKED" && f.info.frame_type !== "BACKFILL");
  const seconds = (toUs - fromUs) / 1e6;
  const count = (pred) => inWin.filter(pred).length;
  const state = (s) => (f) => f.info.state === s;
  const all = count(state("STATE_PRESENTED_ALL")), partial = count(state("STATE_PRESENTED_PARTIAL")), dropped = count(state("STATE_DROPPED")), idle = count(state("STATE_NO_UPDATE_DESIRED"));
  const threads = {};
  for (const f of inWin) { const k = f.info.scroll_state ?? "?"; threads[k] = (threads[k] ?? 0) + 1; }
  const mainScroll = commitsOnly || (threads.SCROLL_MAIN_THREAD ?? 0) > (threads.SCROLL_COMPOSITOR_THREAD ?? 0) + (threads.SCROLL_RASTER ?? 0);
  // The frames in which the list was somewhere new on screen. A scroller the main thread repaints moves only in a
  // presented frame that carries a commit (the reporter has a Commit stage); a reporter that ends PRESENTED_ALL
  // without one is a refresh that owed nothing new from the main thread. A composited scroller moves in every
  // presented frame.
  const presented = (f) => f.info.state === "STATE_PRESENTED_ALL" || f.info.state === "STATE_PRESENTED_PARTIAL";
  // One refresh can have two reporters (the compositor's own frame, and the main thread's update that started in it
  // and is shown some refreshes later), so frames are counted once per picture: by the refresh they began in for a
  // composited scroll, where the compositor's frame is the one that moves the list, and by commit for a scroll the
  // main thread repaints.
  const bySequence = new Map();
  for (const f of inWin.filter(presented)) { const k = f.info.frame_sequence; const had = bySequence.get(k); if (!had || f.end < had.end) bySequence.set(k, f); }
  const moved = mainScroll ? inWin.filter((f) => presented(f) && f.stages.Commit !== undefined) : [...bySequence.values()];
  const shown = [...new Set(moved.map((f) => f.end))].sort((a, b) => a - b);
  const gaps = shown.slice(1).map((t, i) => (t - shown[i]) / 1000);
  const motionMs = shown.length > 1 ? (shown[shown.length - 1] - shown[0]) / 1000 : 0;
  // From the refresh a frame began in to its picture on the display, for the frames that carried a commit.
  const committed = inWin.filter((f) => presented(f) && f.stages.Commit !== undefined);
  const owed = all + partial + dropped;
  const expected = seconds > 0 ? Math.round((seconds * 1000) / intervalMs) : 0;
  const stageDist = {};
  for (const name of STAGES) { const v = inWin.map((f) => f.stages[name]).filter((x) => x !== undefined); if (v.length) stageDist[name] = dist(v); }
  const smoothLost = count((f) => f.info.affects_smoothness && (f.info.state === "STATE_DROPPED" || f.info.state === "STATE_PRESENTED_PARTIAL"));
  return {
    window: { fromUs, toUs },
    seconds: r1(seconds), refreshes: expected, reporters: inWin.length,
    presentedAll: all, presentedPartial: partial, dropped, noUpdateDesired: idle,
    scrollThread: mainScroll ? "main" : (threads.SCROLL_COMPOSITOR_THREAD || threads.SCROLL_RASTER ? "compositor" : "none"), scrollStates: threads,
    // Over the stretch in which the list was moving (first moved picture to last), so a list shorter than the
    // gesture is not charged for the refreshes after it reached its end.
    movedFrames: shown.length, motionSeconds: r1(motionMs / 1000),
    movedFps: motionMs > 0 ? r1(((shown.length - 1) / motionMs) * 1000) : 0,
    movedPctOfRefreshes: motionMs > 0 ? r1(Math.min(100, ((shown.length - 1) / (motionMs / intervalMs)) * 100)) : 0,
    // Every picture presented while the scroll went on, whoever moved the list: for a scroller that has a layer
    // of its own but whose wheel events wait for the main thread, the truth lies between this and movedFps.
    presentedFps: seconds > 0 ? r1(bySequence.size / seconds) : 0,
    lostToSmoothness: smoothLost, lostPct: owed ? r1((smoothLost / owed) * 100) : 0,
    checkerboard: count((f) => f.info.has_missing_content || f.info.checkerboarded_needs_raster || f.info.checkerboarded_needs_record),
    commits: committed.length, commitToGlassMs: dist(committed.map((f) => (f.end - f.begin) / 1000)),
    gapMs: dist(gaps),
    gapsOver: { oneRefresh: gaps.filter((g) => g > intervalMs * 1.5).length, twoRefreshes: gaps.filter((g) => g > intervalMs * 2.5).length, threeRefreshes: gaps.filter((g) => g > intervalMs * 3.5).length, ms17: gaps.filter((g) => g > 17.5).length, ms25: gaps.filter((g) => g > 25).length, ms50: gaps.filter((g) => g > 50).length, ms100: gaps.filter((g) => g > 100).length },
    stagesMs: stageDist,
  };
}

/** Main-thread time by kind of work from `devtools.timeline` events of the renderer's main thread, self time only. */
export function mainThreadBreakdown(events, fromUs, toUs) {
  const main = mainThread(events);
  const KIND = {
    Layout: "layout", UpdateLayoutTree: "style", RecalculateStyles: "style", ScheduleStyleRecalculation: "style", ParseHTML: "other",
    PrePaint: "paint", Paint: "paint", PaintImage: "paint", Layerize: "paint", UpdateLayerTree: "paint", UpdateLayer: "paint", CompositeLayers: "paint", Commit: "commit",
    FunctionCall: "script", EvaluateScript: "script", EventDispatch: "script", FireAnimationFrame: "script", TimerFire: "script", RunMicrotasks: "script", "v8.callFunction": "script", XHRReadyStateChange: "script", FireIdleCallback: "script",
    MajorGC: "gc", MinorGC: "gc", GCEvent: "gc", "V8.GC_SCAVENGER": "gc", "V8.GC_MARK_COMPACTOR": "gc", "BlinkGC.AtomicPhase": "gc",
    HitTest: "hit-test", IntersectionObserverController: "observers",
  };
  const list = events.filter((e) => e.ph === "X" && (!main || (e.pid === main.pid && e.tid === main.tid)) && e.ts >= fromUs && e.ts <= toUs && KIND[e.name] && e.cat.includes("devtools.timeline")).sort((a, b) => a.ts - b.ts || b.dur - a.dur);
  // Self time: each event's duration less its direct children's, walking the events as a stack.
  const settle = [];
  const st = [];
  for (const e of list) {
    while (st.length && st[st.length - 1].end <= e.ts) settle.push(st.pop());
    const dur = e.dur ?? 0;
    if (st.length) st[st.length - 1].child += dur;
    st.push({ end: e.ts + dur, child: 0, kind: KIND[e.name], dur });
  }
  settle.push(...st);
  const out = {};
  for (const n of settle) out[n.kind] = (out[n.kind] ?? 0) + Math.max(0, n.dur - n.child);
  const total = Object.values(out).reduce((a, b) => a + b, 0);
  const res = { totalMs: r1(total / 1000), spanMs: r1((toUs - fromUs) / 1000), pct: {} };
  for (const [k, v] of Object.entries(out).sort((a, b) => b[1] - a[1])) res.pct[k] = r1((v / total) * 100);
  // Raster and GPU work off the main thread, as wall time of their tasks.
  const sum = (name) => events.filter((e) => e.ph === "X" && e.name === name && e.ts >= fromUs && e.ts <= toUs).reduce((a, e) => a + (e.dur ?? 0), 0) / 1000;
  res.rasterTaskMs = r1(sum("RasterTask"));
  res.gpuTaskMs = r1(sum("GPUTask"));
  return res;
}

/** The renderer's main thread: the one named CrRendererMain whose process drew the most frames' style passes. */
function mainThread(events) {
  const named = events.filter((e) => e.ph === "M" && e.name === "thread_name" && e.args?.name === "CrRendererMain");
  if (named.length <= 1) return named[0];
  const busy = (t) => events.filter((e) => e.pid === t.pid && e.tid === t.tid && e.name === "UpdateLayoutTree").length;
  return named.sort((a, b) => busy(b) - busy(a))[0];
}

/** Every style pass of the main thread in the window: how many elements each computed, and how long it took. */
export function stylePasses(events, fromUs, toUs) {
  const main = mainThread(events);
  const passes = events.filter((e) => e.name === "UpdateLayoutTree" && e.ph === "X" && e.ts >= fromUs && e.ts <= toUs && (!main || (e.pid === main.pid && e.tid === main.tid)));
  const elements = dist(passes.map((e) => e.args?.elementCount ?? e.args?.data?.elementCount ?? 0));
  return { n: passes.length, elements, ms: dist(passes.map((e) => (e.dur ?? 0) / 1000)) };
}

/** The main thread's longest tasks in the window (the scheduler's RunTask slices), ms. */
export function longestTasks(events, fromUs, toUs) {
  const main = mainThread(events);
  const tasks = events.filter((e) => e.ph === "X" && (e.name === "RunTask" || e.name === "ThreadControllerImpl::RunTask") && e.ts >= fromUs && e.ts <= toUs && main && e.pid === main.pid && e.tid === main.tid).map((e) => (e.dur ?? 0) / 1000).sort((a, b) => b - a);
  return { maxMs: r1(tasks[0] ?? 0), top: tasks.slice(0, 5).map(r1), over50: tasks.filter((t) => t > 50).length };
}
