/**
 * Settings › Appearance › State colours (#2466, spec #2463) against a ports
 * fake: what each chip sends is what the test reads, and where a chip sits
 * is what the stored answer says.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { KindStateColours, StateWordColour, ToneChoice } from "../../ipc/sources";
import { reset, toasts } from "../../shell/toasts.svelte";
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
  [...(section(name)?.querySelectorAll(`[role="group"][aria-label="${tone}"] .tf-sc-word`) ?? [])].map(text);
const chip = (word: string) => target.querySelector<HTMLElement>(`.tf-sc-chip[aria-label="${word}"]`);
const grip = (word: string) => target.querySelector<HTMLButtonElement>(`button[aria-label="Move ${word}"]`);
const press = (word: string, key: string) => grip(word)!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));

test("each word sits in its tone's column; declared words first, then seen words alphabetically", async () => {
  await draw(fakePorts([tickets()]));

  expect(column("Tickets", "ok")).toEqual(["Done"]);
  expect(column("Tickets", "info")).toEqual(["In Progress"]);
  expect(column("Tickets", "err")).toEqual(["Closed"]);
  expect(column("Tickets", "No colour")).toEqual(["Blocked", "To Do"]);
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
  expect(column("Tickets", "No colour")).toEqual(["Alpha", "Zebra"]);
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
  expect(column("Pull requests", "No colour")).toEqual(["closed"]);

  ports.setToneOverride.mockClear();
  press("closed", "ArrowRight");
  press("merged", "ArrowLeft");
  await settle();
  expect(ports.setToneOverride).not.toHaveBeenCalled();
});

test("a drop on a column sets that column's tone, as the keys do", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);

  chip("Blocked")!.dispatchEvent(new Event("dragstart", { bubbles: true }));
  const err = section("Tickets")!.querySelector('[role="group"][aria-label="err"]')!;
  err.dispatchEvent(new Event("dragover", { bubbles: true, cancelable: true }));
  err.dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
  await settle();

  expect(ports.setToneOverride).toHaveBeenCalledWith("ticket", "Blocked", "err");
  expect(column("Tickets", "err")).toEqual(["Closed", "Blocked"]);

  // A drop back on the column it came from sends nothing.
  ports.setToneOverride.mockClear();
  chip("Done")!.dispatchEvent(new Event("dragstart", { bubbles: true }));
  section("Tickets")!.querySelector('[role="group"][aria-label="ok"]')!.dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
  await settle();
  expect(ports.setToneOverride).not.toHaveBeenCalled();
});

test("a drag that ends without a drop leaves nothing held: a later drop from elsewhere sends nothing", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);

  chip("Blocked")!.dispatchEvent(new Event("dragstart", { bubbles: true }));
  chip("Blocked")!.dispatchEvent(new Event("dragend", { bubbles: true }));
  // Text dragged in from outside the board: no dragstart on a chip.
  section("Tickets")!.querySelector('[role="group"][aria-label="err"]')!.dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
  await settle();
  expect(ports.setToneOverride).not.toHaveBeenCalled();
});

test("a chip says where its tone comes from; Reset sits only on yours and clears that word", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);

  expect(text(chip("Done")!.querySelector(".tf-sc-from"))).toBe("adapter");
  expect(text(chip("Closed")!.querySelector(".tf-sc-from"))).toBe("yours");
  // A to-do status draws no colour, and that is its category's (spec #2463).
  expect(text(chip("To Do")!.querySelector(".tf-sc-from"))).toBe("Jira category");
  expect(target.querySelectorAll('button[aria-label^="Reset "]').length).toBe(1);
  // The word and Reset are the chip's own buttons, the control the design system names for them.
  expect(grip("Closed")!.classList.contains("tf-chipbtn")).toBe(true);
  expect(target.querySelector('button[aria-label="Reset Closed"]')!.classList.contains("tf-chipbtn")).toBe(true);

  target.querySelector<HTMLButtonElement>('button[aria-label="Reset Closed"]')!.click();
  await settle();
  expect(ports.clearToneOverrides).toHaveBeenCalledWith("ticket", "Closed");
  expect(column("Tickets", "muted")).toEqual(["Closed"]);
  expect(target.querySelector('button[aria-label="Reset Closed"]')).toBeNull();
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
  expect(text(chip("Ready for QA")!.querySelector(".tf-sc-from"))).toBe("Jira category");
  expect(column("Tickets", "warn")).toEqual(["Shipped"]);

  target.querySelector<HTMLButtonElement>('button[aria-label="Reset Shipped"]')!.click();
  await settle();
  expect(ports.clearToneOverrides).toHaveBeenCalledWith("ticket", "Shipped");
  expect(column("Tickets", "ok")).toEqual(["Done", "Shipped"]);
  expect(text(chip("Shipped")!.querySelector(".tf-sc-from"))).toBe("Jira category");
});

test("Reset all clears the whole kind", async () => {
  const ports = fakePorts([tickets()]);
  await draw(ports);

  [...section("Tickets")!.querySelectorAll("button")].find((b) => text(b) === "Reset all")!.click();
  await settle();
  expect(ports.clearToneOverrides).toHaveBeenCalledWith("ticket", null);
});

test("a failing set raises a toast and leaves the chip where the stored answer puts it", async () => {
  const ports = fakePorts([tickets()], {
    setToneOverride: vi.fn(() => Promise.reject({ code: "not_ready", message: "the database is not up yet", source_id: null })),
  });
  await draw(ports);

  press("Done", "ArrowRight");
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
