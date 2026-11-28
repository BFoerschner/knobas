/**
 * The run row's live duration cell (#2574, the mockup's *States, and what holds still*): a run in flight counts up where
 * its duration will stand, the bar of the pipeline's usual time filling in the info tone, and its end draws the same bar
 * with what it took. Queued, it waits.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { RunRow } from "../../ipc/entity";
import { runsRead } from "./runs-read.svelte";
import TfRunRow from "./TfRunRow.svelte";

let body: HTMLElement;
let row: ReturnType<typeof mount> | null = null;
const NOW = new Date("2026-11-28T10:00:00Z").getTime();

beforeEach(() => {
  vi.useFakeTimers({ now: NOW });
  const table = document.createElement("table");
  body = document.createElement("tbody");
  table.appendChild(body);
  document.body.appendChild(table);
});

afterEach(() => {
  if (row) unmount(row);
  row = null;
  body.parentElement?.remove();
  runsRead.forget();
  vi.useRealTimers();
});

const ago = (seconds: number) => new Date(NOW - seconds * 1000).toISOString();

function runRow(extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: "tc:build:7",
    number: "7",
    state: { text: "RUNNING", tone: null },
    branch: "main",
    started: ago(125),
    finished: null,
    duration_seconds: null,
    updated_at: ago(125),
    web_url: null,
    trigger: null,
    started_by: null,
    started_by_me: false,
    revision: null,
    commit_id: null,
    agent: null,
    current_step: null,
    tests: null,
    ...extra,
  };
}

/** Draws the row; `set` changes its run, as a read that moved it does. */
function draw(first: RunRow, scale: { max: number; usual: number | null }) {
  let run = $state.raw(first);
  row = mount(TfRunRow, {
    target: body,
    props: {
      get run() {
        return run;
      },
      attach: () => {},
      href: "#/build/p/7",
      max: scale.max,
      usual: scale.usual,
      onopen: () => {},
      onagain: () => {},
    },
  });
  flushSync();
  return (next: RunRow) => {
    run = next;
    flushSync();
  };
}

const cell = () => body.querySelector<HTMLElement>(".tf-run-dur")!;
const said = () => cell().textContent?.trim();
const width = (sel: string) => cell().querySelector(sel)?.getAttribute("width") ?? null;

test("a running run's clock ticks each second where its duration will stand, in tabular figures (#2574)", () => {
  draw(runRow(), { max: 600, usual: 300 });
  expect(said()).toBe("2m 05s");
  vi.advanceTimersByTime(1000);
  flushSync();
  expect(said()).toBe("2m 06s");
  vi.advanceTimersByTime(4000);
  flushSync();
  expect(said()).toBe("2m 10s");
  expect(cell().querySelector(".tf-progress")).toBeNull();
});

test("the bar of the pipeline's usual time fills as it runs, on the scale its finished runs' bars stand on (#2574)", () => {
  // Scale: the longest run held, 600 s over the bar's 60 px. The usual time (the median, 300 s) is the track, 30 px.
  draw(runRow({ started: ago(120) }), { max: 600, usual: 300 });
  expect(width(".tf-durbar-usual")).toBe("30");
  expect(width(".tf-durbar-fill")).toBe("12");
  vi.advanceTimersByTime(60_000);
  flushSync();
  expect(width(".tf-durbar-fill")).toBe("18");
  // Past the longest run held, the fill stops at the bar's end.
  vi.advanceTimersByTime(900_000);
  flushSync();
  expect(width(".tf-durbar-fill")).toBe("60");
});

test("with no finished run to say the usual time, the bar's room stands empty and the clock ticks (#2574)", () => {
  draw(runRow({ started: ago(5) }), { max: 0, usual: null });
  expect(cell().querySelector("svg.tf-durbar")?.getAttribute("width")).toBe("60");
  expect(cell().querySelector("svg.tf-durbar rect")).toBeNull();
  expect(said()).toBe("5s");
});

test("queued it waits; started it ticks; ended, the same bar and the words say what it took (#2574)", () => {
  const set = draw(runRow({ number: null, state: { text: "queued", tone: null }, started: null, updated_at: ago(30) }), { max: 600, usual: 300 });
  expect(said()).toBe("waiting");
  set(runRow({ started: ago(250) }));
  expect(said()).toBe("4m 10s");
  expect(width(".tf-durbar-fill")).toBe("25");
  set(runRow({ started: ago(252), finished: ago(0), duration_seconds: 252, state: { text: "SUCCESS", tone: "ok" } }));
  expect(said()).toBe("4m 12s");
  expect(cell().querySelector(".tf-durbar-fill")).toBeNull();
  expect(cell().querySelector("svg.tf-durbar rect")?.getAttribute("width")).toBe("25");
  // Ended, it ticks no more.
  vi.advanceTimersByTime(5000);
  flushSync();
  expect(said()).toBe("4m 12s");
});

test("running and ended, the cell says a length in one form, the smaller unit in two figures (#2574)", () => {
  const set = draw(runRow({ started: ago(3725) }), { max: 7200, usual: 3600 });
  expect(said()).toBe("1h 02m");
  set(runRow({ started: ago(3725), finished: ago(0), duration_seconds: 3725, state: { text: "SUCCESS", tone: "ok" } }));
  expect(said(), "not secondsLabel's 1h 2m").toBe("1h 02m");
  set(runRow({ started: ago(245), finished: ago(0), duration_seconds: 245, state: { text: "SUCCESS", tone: "ok" } }));
  expect(said()).toBe("4m 05s");
});
