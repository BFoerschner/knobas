/**
 * Reads started while the pointer rests on a link (the inner-pages pass, 2026-11-12): which links read what, that a
 * pointer must stand a moment and one swept past asks nothing, that each is asked once, and that the page's own ask
 * after the press joins the read under way rather than reaching the source twice.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { entityView, readViewAhead } from "../../ipc/entity";
import { installHoverReads, readOf, REST_MS } from "./hover-reads";

const sent: { cmd: string; args: unknown }[] = [];
let answers: ((value: unknown) => void)[] = [];

vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/core")>()),
  invoke: (cmd: string, args: unknown) => {
    sent.push({ cmd, args });
    return new Promise((resolve) => answers.push(resolve));
  },
}));

let root: HTMLElement;
let stop: () => void;
beforeEach(() => {
  vi.useFakeTimers();
  sent.length = 0;
  answers = [];
  root = document.createElement("div");
  root.innerHTML = `<a id="repo" href="#/repo/gitea%3Atidewater%2Fpayout-service">repo</a>
    <a id="files" href="#/pr/gitea%3Atidewater%2Fpayout-service%23142/diff">files</a>
    <a id="tab" href="#/repo/gitea%3Atidewater%2Fpayout-service/commits">commits</a>
    <a id="other" href="#/tickets">tickets</a>`;
  document.body.append(root);
  stop = installHoverReads(root);
});
afterEach(() => {
  stop();
  root.remove();
  vi.useRealTimers();
});

/** The app's `invoke` reaches the bridge a few microtasks after it is called. */
const flush = async () => {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
};
const over = (id: string) => root.querySelector(`#${id}`)!.dispatchEvent(new Event("pointerover", { bubbles: true }));

test("a repository reads its top directory, a pull request's Files changed its files, and nothing else reads", () => {
  expect(readOf("#/repo/gitea%3Atidewater%2Fpayout-service")).toEqual({ entityId: "gitea:tidewater/payout-service", section: "files" });
  expect(readOf("#/pr/gitea%3Atidewater%2Fpayout-service%23142/diff")).toEqual({ entityId: "gitea:tidewater/payout-service#142", section: "files" });
  expect(readOf("#/repo/gitea%3Atidewater%2Fpayout-service/commits")).toBeNull();
  expect(readOf("#/pr/gitea%3Atidewater%2Fpayout-service%23142")).toBeNull();
  expect(readOf("#/tickets")).toBeNull();
});

test("the read starts once the pointer has stood on the link, and a pointer swept past asks nothing", async () => {
  over("repo");
  vi.advanceTimersByTime(REST_MS - 10);
  over("other");
  vi.advanceTimersByTime(REST_MS * 2);
  await flush();
  expect(sent).toEqual([]);
  over("tab");
  vi.advanceTimersByTime(REST_MS * 2);
  await flush();
  expect(sent).toEqual([]);
  over("files");
  vi.advanceTimersByTime(REST_MS);
  await flush();
  expect(sent).toEqual([{ cmd: "entity_view", args: { entityId: "gitea:tidewater/payout-service#142", section: "files" } }]);
});

test("each link is read once a session", async () => {
  over("repo");
  vi.advanceTimersByTime(REST_MS);
  over("other");
  over("repo");
  vi.advanceTimersByTime(REST_MS);
  await flush();
  expect(sent).toHaveLength(1);
});

test("the page's ask after the press joins the read under way, and one after it lands asks again", async () => {
  vi.useRealTimers();
  // Another repository than the tests above, whose reads ahead are still out.
  const ahead = readViewAhead("gitea:tidewater/ledger-api", "files");
  const page = entityView("gitea:tidewater/ledger-api", "files");
  await flush();
  expect(sent).toHaveLength(1);
  const view = { sections: [], actions: [] };
  answers[0]!(view);
  expect(await ahead).toEqual(view);
  expect(await page).toEqual(view);
  void entityView("gitea:tidewater/ledger-api", "files");
  await flush();
  expect(sent).toHaveLength(2);
});
