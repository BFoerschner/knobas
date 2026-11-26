/**
 * Settings › Appearance › State colours (#2466, spec #2463) against a ports
 * fake: what each chip sends is what the test reads, and where a chip sits
 * is what the stored answer says.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { KindStateColours, StateWordColour, ToneChoice } from "../../ipc/sources";
import { reset, toasts } from "../../shell/toasts.svelte";
import { stateWord } from "../state-word";
import { columnName } from "./state-colours";
import TfStateColours, { type StateColourPorts } from "./TfStateColours.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  reset();
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

function word(over: Partial<StateWordColour> & { word: string }): StateWordColour {
  const tone = over.tone === undefined ? (over.declared_tone ?? null) : over.tone;
  return { declared: true, declared_tone: null, yours: null, origin: "adapter", ...over, category_tone: over.category_tone ?? null, tone };
}

/** A ticket kind as the read answers it, but with the seen words out of order. */
function tickets(): KindStateColours {
  return {
    kind: "ticket",
    has_category: true,
    words: [
      word({ word: "To Do", declared: false, origin: "category" }),
      word({ word: "Done", declared_tone: "ok" }),
      word({ word: "Blocked", declared: false, origin: "category" }),
      word({ word: "In Progress", declared_tone: "info" }),
      word({ word: "Closed", declared_tone: "muted", yours: "err", tone: "err", origin: "yours" }),
    ],
  };
}

/** The fake applies a set or a clear to its stored answer, as the command does. */
function fakePorts(start: KindStateColours[], over: Partial<StateColourPorts> = {}) {
  let stored = start;
  const apply = (kind: string, pick: (w: StateWordColour) => boolean, yours: ToneChoice | null) => {
    stored = stored.map((k) =>
      k.kind !== kind
        ? k
        : {
            ...k,
            words: k.words.map((w) =>
              !pick(w)
                ? w
                : {
                    ...w,
                    yours,
                    tone: yours === null ? (w.declared ? w.declared_tone : w.category_tone) : yours === "none" ? null : yours,
                    origin: yours === null ? (w.declared ? "adapter" : k.has_category ? "category" : "unset") : "yours",
                  },
            ),
          },
    );
    return Promise.resolve(stored);
  };
  const ports = {
    stateColours: vi.fn<StateColourPorts["stateColours"]>(() => Promise.resolve(stored)),
    setToneOverride: vi.fn<StateColourPorts["setToneOverride"]>((kind, w, tone) => apply(kind, (x) => x.word === w, tone)),
    clearToneOverrides: vi.fn<StateColourPorts["clearToneOverrides"]>((kind, w) => apply(kind, (x) => w === null || x.word === w, null)),
  };
  return Object.assign(ports, over) as typeof ports;
}

async function draw(ports: StateColourPorts) {
  view = mount(TfStateColours, { target, props: { ports } });
  await settle();
}

const section = (name: string) => target.querySelector<HTMLElement>(`section[aria-label="${name}"]`);
/** The words in a section's column, in the order drawn. */
const column = (name: string, tone: string) =>
  [...(section(name)?.querySelectorAll(`[role="group"][aria-label="${columnName(tone as ToneChoice)}"] .tf-sc-word`) ?? [])].map(text);
const chip = (word: string) => target.querySelector<HTMLElement>(`.tf-sc-chip[aria-label="${stateWord(word)}"]`);
const grip = (word: string) => target.querySelector<HTMLButtonElement>(`button[aria-label="Colour of ${stateWord(word)}"]`);
const press = (word: string, key: string) => grip(word)!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));

test("each word sits in its tone's column; declared words first, then seen words alphabetically", async () => {
  await draw(fakePorts([tickets()]));

  expect(column("Tickets", "ok")).toEqual(["Done"]);
  expect(column("Tickets", "info")).toEqual(["In progress"]);
  expect(column("Tickets", "err")).toEqual(["Closed"]);
  expect(column("Tickets", "none")).toEqual(["Blocked", "To do"]);
  expect(column("Tickets", "warn")).toEqual([]);
  expect(column("Tickets", "muted")).toEqual([]);
  // Across the board, in reading order: the declared ones in declared order, then the seen ones.
  const all = [...section("Tickets")!.querySelectorAll(".tf-sc-word")].map(text);
  expect(all.indexOf("Done")).toBeLessThan(all.indexOf("Blocked"));
  // A chip draws in its tone's colour.
  expect(chip("Done")!.classList.contains("tf-ok")).toBe(true);
  expect(chip("Blocked")!.className).not.toMatch(/tf-(ok|warn|err|info|muted)\b/);
});

test("the order puts declared words before seen ones whatever order the read gives", async () => {
  const mixed: KindStateColours = {
    kind: "ticket",
    has_category: true,
    words: [
      word({ word: "Zebra", declared: false, origin: "unset" }),
      word({ word: "Waiting", declared_tone: "warn" }),
      word({ word: "Alpha", declared: false, origin: "unset" }),
      word({ word: "Paused", declared_tone: "warn" }),
    ],
  };
  await draw(fakePorts([mixed]));

  expect(column("Tickets", "warn")).toEqual(["Waiting", "Paused"]);
  expect(column("Tickets", "none")).toEqual(["Alpha", "Zebra"]);
});

test("→ moves a focused chip one column and sets the tone; ← from info sets err", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);

  press("Done", "ArrowRight");
  await settle();
  expect(ports.setToneOverride).toHaveBeenLastCalledWith("ticket", "Done", "warn");
  expect(column("Tickets", "warn")).toEqual(["Done"]);
  // The chip keeps the focus in its new column, so the next press moves it again.
  expect(document.activeElement).toBe(grip("Done"));

  press("In Progress", "ArrowLeft");
  await settle();
  expect(ports.setToneOverride).toHaveBeenLastCalledWith("ticket", "In Progress", "err");
});

test("→ from muted sets No colour; → at No colour and ← at ok do nothing", async () => {
  const ports = fakePorts([
    {
      kind: "pr",
      has_category: false,
      words: [word({ word: "closed", declared_tone: "muted" }), word({ word: "merged", declared_tone: "ok" })],
    },
  ]);
  await draw(ports);

  press("closed", "ArrowRight");
  await settle();
  expect(ports.setToneOverride).toHaveBeenLastCalledWith("pr", "closed", "none");
  expect(column("Pull requests", "none")).toEqual(["Closed"]);

  ports.setToneOverride.mockClear();
  press("closed", "ArrowRight");
  press("merged", "ArrowLeft");
  await settle();
  expect(ports.setToneOverride).not.toHaveBeenCalled();
});

/** A pointer event as a person's: the primary pointer, its main button. */
function pointer(type: string, x: number, y: number): Event {
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 });
  Object.defineProperties(event, { pointerId: { value: 1 }, isPrimary: { value: true } });
  return event;
}

const columnEl = (name: string, tone: ToneChoice) => section(name)!.querySelector<HTMLElement>(`[role="group"][aria-label="${columnName(tone)}"]`)!;

/** Press a chip's word, move past a tremble to where the hit test answers `onto`, and let go there; then the click a release makes. */
function dragOnto(word: string, onto: Element | null) {
  document.elementsFromPoint = () => (onto === null ? [] : [onto]);
  const handle = grip(word)!;
  // As a real pointer: past its first move it is over the board, not the chip, until it is captured.
  const elsewhere = onto ?? document.body;
  handle.dispatchEvent(pointer("pointerdown", 10, 10));
  elsewhere.dispatchEvent(pointer("pointermove", 60, 14));
  const lifted = chip(word)!.classList.contains("tf-lifted");
  const highlighted = onto?.closest(".tf-sc-col")?.classList.contains("tf-sc-over") ?? false;
  elsewhere.dispatchEvent(pointer("pointerup", 60, 14));
  handle.click();
  return { lifted, highlighted };
}

test("a chip dragged by pointer onto a column sets that column's tone, and the release opens no menu", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);

  const seen = dragOnto("Blocked", columnEl("Tickets", "err").querySelector(".tf-sc-colh"));
  flushSync();
  expect(seen).toEqual({ lifted: true, highlighted: false });
  // Drawn at once, before the write answers.
  expect(column("Tickets", "err")).toEqual(["Closed", "Blocked"]);
  await settle();
  expect(ports.setToneOverride).toHaveBeenCalledWith("ticket", "Blocked", "err");
  expect(column("Tickets", "err")).toEqual(["Closed", "Blocked"]);
  expect(document.querySelector('[role="menu"]')).toBeNull();
  expect(chip("Blocked")!.classList.contains("tf-lifted")).toBe(false);

  // A drop back on its own column, or on another kind's column, or on nothing sends nothing.
  ports.setToneOverride.mockClear();
  dragOnto("Done", columnEl("Tickets", "ok"));
  await settle();
  expect(document.querySelector('[role="menu"]'), "the click a drag's release makes is no press").toBeNull();
  dragOnto("Done", null);
  await settle();
  expect(ports.setToneOverride).not.toHaveBeenCalled();
  expect(document.querySelector('[role="menu"]')).toBeNull();
});

test("a drag shows where it would land: the column under the pointer is marked while the chip is lifted", async () => {
  await draw(fakePorts([tickets()]));
  document.elementsFromPoint = () => [columnEl("Tickets", "warn")];
  grip("Done")!.dispatchEvent(pointer("pointerdown", 10, 10));
  grip("Done")!.dispatchEvent(pointer("pointermove", 60, 14));
  flushSync();
  expect(columnEl("Tickets", "warn").classList.contains("tf-sc-over")).toBe(true);
  expect(chip("Done")!.style.transform).toBe("translate(50px, 4px)");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  expect(columnEl("Tickets", "warn").classList.contains("tf-sc-over")).toBe(false);
  expect(chip("Done")!.style.transform).toBe("");
});

test("a press on a word opens a menu of the six columns, its own checked; a pick lands it there and focus stays with it", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);

  expect(grip("Blocked")!.getAttribute("aria-haspopup")).toBe("menu");
  grip("Blocked")!.click();
  await settle();
  const menu = document.querySelector<HTMLElement>('[role="menu"]')!;
  const items = [...menu.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')];
  expect(items.map(text)).toEqual(["Good", "Warning", "Error", "Active", "Quiet", "No colour✓"]);
  expect(items.map((i) => i.getAttribute("aria-checked"))).toEqual(["false", "false", "false", "false", "false", "true"]);
  expect(grip("Blocked")!.getAttribute("aria-expanded")).toBe("true");

  items[3]!.click();
  flushSync();
  expect(column("Tickets", "info")).toEqual(["In progress", "Blocked"]);
  await settle();
  expect(ports.setToneOverride).toHaveBeenCalledWith("ticket", "Blocked", "info");
  expect(document.querySelector('[role="menu"]')).toBeNull();
  expect(document.activeElement).toBe(grip("Blocked"));
});

test("a pick of the column a word is in sends nothing", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);
  grip("Done")!.click();
  await settle();
  [...document.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')].find((i) => text(i).startsWith("Good"))!.click();
  await settle();
  expect(ports.setToneOverride).not.toHaveBeenCalled();
});

test("a move is drawn before its write answers, and presses while one is out are kept, the newest target of a word sent once", async () => {
  const ports = fakePorts([tickets()]);
  const answers: (() => void)[] = [];
  const real = ports.setToneOverride.getMockImplementation()!;
  ports.setToneOverride.mockImplementation(
    (kind, w, tone) => new Promise((resolve) => answers.push(() => void real(kind, w, tone).then(resolve))),
  );
  await draw(ports);

  press("Done", "ArrowRight");
  flushSync();
  expect(column("Tickets", "warn")).toEqual(["Done"]);
  // Three more presses while the first write is out: none is dropped.
  press("Done", "ArrowRight");
  press("Done", "ArrowRight");
  press("Done", "ArrowRight");
  flushSync();
  expect(column("Tickets", "muted")).toEqual(["Done"]);
  expect(ports.setToneOverride).toHaveBeenCalledTimes(1);

  answers.shift()!();
  await settle();
  // The first answer (warn) is stored, and the waiting move is still drawn over it: the chip never steps back.
  expect(column("Tickets", "muted")).toEqual(["Done"]);
  expect(ports.setToneOverride).toHaveBeenCalledTimes(2);
  expect(ports.setToneOverride).toHaveBeenLastCalledWith("ticket", "Done", "muted");
  answers.shift()!();
  await settle();
  expect(column("Tickets", "muted")).toEqual(["Done"]);
  expect(ports.setToneOverride).toHaveBeenCalledTimes(2);
});

test("only a word you moved is marked; where the others come from is in the title; Reset clears that word (F-634)", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);

  expect(text(chip("Done"))).toBe("Done");
  expect(grip("Done")!.title).toContain("From the adapter");
  expect(text(chip("Closed"))).toBe("ClosedReset");
  expect(grip("Closed")!.title).toContain("Yours");
  // A to-do status draws no colour, and that is its category's (spec #2463).
  expect(text(chip("To Do"))).toBe("To do");
  expect(grip("To Do")!.title).toContain("From its Jira category");
  expect(target.querySelectorAll('button[aria-label^="Reset "]').length).toBe(1);
  // The word and Reset are the chip's own buttons, the control the design system names for them.
  expect(grip("Closed")!.classList.contains("tf-chipbtn")).toBe(true);
  expect(target.querySelector('button[aria-label="Reset Closed"]')!.classList.contains("tf-chipbtn")).toBe(true);

  target.querySelector<HTMLButtonElement>('button[aria-label="Reset Closed"]')!.click();
  await settle();
  expect(ports.clearToneOverrides).toHaveBeenCalledWith("ticket", "Closed");
  expect(column("Tickets", "muted")).toEqual(["Closed"]);
  expect(target.querySelector('button[aria-label="Reset Closed"]')).toBeNull();
  expect(document.activeElement, "focus stays with the word, which moved").toBe(grip("Closed"));
});

test("a seen status at its category's tone sits in that column, labelled Jira category; Reset returns it there", async () => {
  const ports = fakePorts([
    {
      kind: "ticket",
      has_category: true,
      words: [
        word({ word: "Done", declared_tone: "ok" }),
        word({ word: "Ready for QA", declared: false, category_tone: "info", tone: "info", origin: "category" }),
        word({ word: "Shipped", declared: false, category_tone: "ok", yours: "warn", tone: "warn", origin: "yours" }),
      ],
    },
  ]);
  await draw(ports);

  expect(column("Tickets", "info")).toEqual(["Ready for QA"]);
  expect(grip("Ready for QA")!.title).toContain("From its Jira category");
  expect(column("Tickets", "warn")).toEqual(["Shipped"]);

  target.querySelector<HTMLButtonElement>('button[aria-label="Reset Shipped"]')!.click();
  await settle();
  expect(ports.clearToneOverrides).toHaveBeenCalledWith("ticket", "Shipped");
  expect(column("Tickets", "ok")).toEqual(["Done", "Shipped"]);
  expect(grip("Shipped")!.title).toContain("From its Jira category");
  expect(text(chip("Shipped"))).toBe("Shipped");
});

test("a chip says the word as other chips do, the source's spelling in its title, and sends the source's word (F-634)", async () => {
  const ports = fakePorts([{ kind: "build", has_category: false, words: [word({ word: "SUCCESS", declared_tone: "ok" }), word({ word: "FAILURE", declared_tone: "err" })] }]);
  await draw(ports);

  expect(column("Builds", "ok")).toEqual(["Passed"]);
  expect(column("Builds", "err")).toEqual(["Failed"]);
  expect(grip("SUCCESS")!.title).toContain("SUCCESS");
  // What is stored is the source's word.
  press("SUCCESS", "ArrowRight");
  await settle();
  expect(ports.setToneOverride).toHaveBeenCalledWith("build", "SUCCESS", "warn");
  expect(column("Builds", "warn")).toEqual(["Passed"]);
  expect(document.activeElement).toBe(grip("SUCCESS"));
});

test("a column is headed by what it means, and the line under a section names columns, not tokens (F-634)", async () => {
  await draw(fakePorts([{ kind: "build", has_category: false, words: [word({ word: "SUCCESS", declared_tone: "ok" })] }]));

  const heads = [...section("Builds")!.querySelectorAll(".tf-sc-colh")].map(text);
  expect(heads).toEqual(["Good", "Warning", "Error", "Active", "Quiet", "No colour"]);
  const line = text(section("Builds")!.querySelector(".tf-sc-line"));
  expect(line).toContain("Error is what the inbox and signals count as a failed build");
  expect(line).not.toMatch(/\b(err|ok|info|muted)\b/);
});

test("Reset all clears the whole kind", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);

  [...section("Tickets")!.querySelectorAll("button")].find((b) => text(b) === "Reset all")!.click();
  flushSync();
  expect(column("Tickets", "muted"), "drawn at once").toEqual(["Closed"]);
  await settle();
  expect(ports.clearToneOverrides).toHaveBeenCalledWith("ticket", null);
  expect(column("Tickets", "muted")).toEqual(["Closed"]);
});

test("a failing set raises a toast and leaves the chip where the stored answer puts it", async () => {
  const ports = fakePorts([tickets()], {
    setToneOverride: vi.fn(() => Promise.reject({ code: "not_ready", message: "the database is not up yet", source_id: null })),
  });
  await draw(ports);

  press("Done", "ArrowRight");
  flushSync();
  expect(column("Tickets", "warn")).toEqual(["Done"]);
  await settle();
  expect(column("Tickets", "ok")).toEqual(["Done"]);
  expect(column("Tickets", "warn")).toEqual([]);
  expect(toasts.items.map((t) => [t.text, t.tone])).toEqual([["Could not set the colour of Done: the database is not up yet", "err"]]);
});

test("a failing read raises a toast and draws no board", async () => {
  await draw(fakePorts([], { stateColours: vi.fn(() => Promise.reject(new Error("gone"))) }));

  expect(target.querySelector("section")).toBeNull();
  expect(toasts.items.map((t) => t.tone)).toEqual(["err"]);
});
