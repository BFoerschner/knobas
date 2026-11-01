/**
 * The two marks on the Tickets board and list (spec #2396, #2398), through
 * the app's own mount over the whole fake IPC: yellow (`tf-card`) follows the
 * chosen job, selected or not; the selection (`aria-current`, `tf-sel`) is an
 * outline, and on the board the lift. Both on one card when they coincide. It
 * lives under `dev/` because only the harness may import the harness
 * (`house-rules.test.ts`).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import App from "../../../App.svelte";
import { CHOSEN_KEY, chosenJob } from "../../tf/job/chosen.svelte";
import { demoHandlers, installFakeTauri } from "./fake-tauri";
import { until } from "../test-wait";

const CHOSEN = "mock:PAY-231";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/** The webview's storage, where the shell's mount reads the chosen job from. */
function memoryStorage(): Storage {
  const kept = new Map<string, string>();
  return {
    get length() {
      return kept.size;
    },
    clear: () => kept.clear(),
    getItem: (k) => kept.get(k) ?? null,
    key: (i) => [...kept.keys()][i] ?? null,
    removeItem: (k) => void kept.delete(k),
    setItem: (k, v) => void kept.set(k, v),
  };
}

beforeEach(() => {
  installFakeTauri(demoHandlers());
  vi.stubGlobal("localStorage", memoryStorage());
  localStorage.setItem(CHOSEN_KEY, CHOSEN);
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  chosenJob.reset();
  vi.unstubAllGlobals();
  history.replaceState(null, "", location.pathname + location.search);
});

/** What the shown page draws: none of what keep-alive holds hidden (#2247). */
function shown<E extends HTMLElement = HTMLElement>(selector: string): E[] {
  return [...target.querySelectorAll<E>(`.tf-kept:not([hidden]) ${selector}`)];
}

async function board(): Promise<void> {
  location.hash = "#/tickets/payout-reliability";
  app = mount(App, { target, props: {} });
  await until(() => shown(`.tf-tcard[data-entity="${CHOSEN}"]`).length > 0, "the board never drew PAY-231");
}

async function list(): Promise<void> {
  await board();
  shown<HTMLButtonElement>('[aria-label="Layout"] button')
    .find((b) => b.textContent === "List")!
    .click();
  await until(() => shown(`.tf-row[data-entity="${CHOSEN}"]`).length > 0, "the list never drew PAY-231");
}

/** Each mark, by the entity ids that carry it. */
function marks(selector: string): { yellow: string[]; selected: string[]; outlined: string[] } {
  const ids = (extra: string) => shown(`${selector}${extra}`).map((el) => el.dataset.entity!);
  return { yellow: ids(".tf-card"), selected: ids('[aria-current="true"]'), outlined: ids(".tf-sel") };
}

/** A ticket on screen that is not the chosen one. */
function other(selector: string): HTMLElement {
  return shown(selector).find((el) => el.dataset.entity !== CHOSEN)!;
}

test("on the board, the chosen job's card is yellow while another card is selected, and only the selected card is outlined", async () => {
  await board();
  const another = other(".tf-tcard");
  another.click();
  flushSync();
  await until(() => another.getAttribute("aria-current") === "true", "the click never selected the card");

  expect(marks(".tf-tcard")).toEqual({ yellow: [CHOSEN], selected: [another.dataset.entity], outlined: [another.dataset.entity] });
});

test("on the board, the chosen job's card selected is yellow and outlined at once", async () => {
  await board();
  const chosen = shown(`.tf-tcard[data-entity="${CHOSEN}"]`)[0]!;
  chosen.click();
  flushSync();
  await until(() => chosen.getAttribute("aria-current") === "true", "the click never selected PAY-231");

  expect(marks(".tf-tcard")).toEqual({ yellow: [CHOSEN], selected: [CHOSEN], outlined: [CHOSEN] });
});

test("in the list, the chosen job's row is yellow whichever row is selected; the selected row is outlined", async () => {
  await list();
  const another = other(".tf-row");
  another.click();
  flushSync();
  await until(() => another.getAttribute("aria-current") === "true", "the click never selected the row");
  expect(marks(".tf-row")).toEqual({ yellow: [CHOSEN], selected: [another.dataset.entity], outlined: [another.dataset.entity] });

  const chosen = shown(`.tf-row[data-entity="${CHOSEN}"]`)[0]!;
  chosen.click();
  flushSync();
  await until(() => chosen.getAttribute("aria-current") === "true", "the click never selected PAY-231");
  expect(marks(".tf-row")).toEqual({ yellow: [CHOSEN], selected: [CHOSEN], outlined: [CHOSEN] });
});

test("with no job chosen, nothing on the board is yellow", async () => {
  localStorage.removeItem(CHOSEN_KEY);
  await board();
  expect(marks(".tf-tcard").yellow).toEqual([]);
  expect(marks(".tf-tcard").selected).toHaveLength(1);
});
