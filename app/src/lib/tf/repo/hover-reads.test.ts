/**
 * Reads started while the pointer rests on a link (the inner-pages pass, 2026-11-12): which links read what, that a
 * pointer must stand a moment and one swept past asks nothing, that each is asked once, and that the page's own ask
 * after the press joins the read under way rather than reaching the source twice.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { entityView, readViewAhead } from "../../ipc/entity";
import { forgetLandingReads, holdRepoPage, installHoverReads, landingGuess, readLandingAtAddress, readOf, REST_MS } from "./hover-reads";

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
  forgetLandingReads();
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

test("a press before the pointer has stood its moment asks nothing more: the page's own read is the read", async () => {
  // The WSL block, 2026-11-14: a press within the rest, as the open budget's and a quick hand's, left the rest's read
  // to go out 80 ms later beside the page's, and a pull request of 3,000 files read its 8 MB from Gitea twice.
  over("files");
  vi.advanceTimersByTime(REST_MS - 70);
  root.querySelector("#files")!.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  vi.advanceTimersByTime(REST_MS * 2);
  await flush();
  expect(sent).toEqual([]);
  // Nor later in the session: the backend holds what the page read.
  over("other");
  over("files");
  vi.advanceTimersByTime(REST_MS * 2);
  await flush();
  expect(sent).toEqual([]);
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

// The ux14 pass (2026-11-16): a repository's first open sent its reads 118 and 166 ms after the press at CPU 4x and 6x,
// behind the build of the page. The press sends them, and the page's asks take them.
test("a press on a repository's link reads its landing at once, and the page's asks take those reads", async () => {
  vi.useRealTimers();
  root.querySelector("#repo")!.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  await flush();
  const id = "gitea:tidewater/payout-service";
  expect(sent).toEqual([
    { cmd: "get_entity", args: { entityId: id } },
    { cmd: "code_facts", args: { entityIds: [id] } },
    { cmd: "repo_branch_pulls", args: { repoId: id } },
    { cmd: "repo_pipelines", args: { repoId: id } },
    { cmd: "entity_view", args: { entityId: id, section: "files" } },
    { cmd: "entity_view", args: { entityId: id, section: "commits" } },
    { cmd: "entity_view", args: { entityId: id, section: "branches" } },
    // The header's checkout (the ux30 end gate, Windows).
    { cmd: "entity_checkout", args: { entityId: id } },
  ]);
  const view = { sections: [], actions: [] };
  for (const answer of answers.slice(4)) answer(view);
  await flush();
  // Landed before the page asked: taken once, and an ask after that reads again.
  expect(await entityView(id, "files")).toEqual(view);
  await flush();
  expect(sent).toHaveLength(8);
  void entityView(id, "files");
  await flush();
  expect(sent).toHaveLength(9);
  // A tab's link (`/commits`) and a pull request's link read nothing at the press.
  sent.length = 0;
  root.querySelector("#tab")!.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  await flush();
  expect(sent).toEqual([]);
});

// The ux16 overs pass (2026-11-17): the landing's README guess was a second round after its listing, behind the page's
// build (130 ms more at 6x on Björn's PC). The press asks it once the record names the source's tree sections.
test("a press asks the landing's README guess once the record names the tree's sections", async () => {
  vi.useRealTimers();
  root.innerHTML = `<a id="readme" href="#/repo/gitea%3Atidewater%2Freadme-service">repo</a>`;
  root.querySelector("#readme")!.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  await flush();
  const id = "gitea:tidewater/readme-service";
  expect(landingGuess(id), "not before the record says how the source names a file").toBeNull();
  answers[0]!({ tree: { dir: "files:", file: "file:" } });
  await flush();
  expect(sent.at(-1)).toEqual({ cmd: "entity_view", args: { entityId: id, section: "file:README.md" } });
  expect(landingGuess(id)).toBe("file:README.md");
  // The landing's own ask of it takes the read under way.
  const n = sent.length;
  void entityView(id, "file:README.md");
  await flush();
  expect(sent).toHaveLength(n);
});

test("Enter on a repository's link reads its landing as a press does", async () => {
  vi.useRealTimers();
  root.querySelector("#repo")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await flush();
  expect(sent.map((s) => s.cmd)).toContain("get_entity");
});

// The compare-first-open pass (2026-11-20, Fable's ruling): an address arrival (a launcher jump) read the landing only once
// the page was built, 118 and 166 ms after the address at 4x and 6x before the press got its own reads.
test("an address on a repository's landing reads it before the page is built; a tab's address reads nothing", async () => {
  vi.useRealTimers();
  readLandingAtAddress("#/repo/gitea%3Atidewater%2Fpayout-service/commits");
  await flush();
  expect(sent).toEqual([]);
  readLandingAtAddress("#/repo/gitea%3Atidewater%2Fpayout-service");
  await flush();
  expect(sent.map((s) => s.cmd)).toEqual(["get_entity", "code_facts", "repo_branch_pulls", "repo_pipelines", "entity_view", "entity_view", "entity_view", "entity_checkout"]);
});

test("a press and the address it sets read the landing once", async () => {
  vi.useRealTimers();
  root.querySelector("#repo")!.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  await flush();
  const n = sent.length;
  expect(n).toBeGreaterThan(0);
  readLandingAtAddress("#/repo/gitea%3Atidewater%2Fpayout-service");
  await flush();
  expect(sent).toHaveLength(n);
});

test("a repository a page holds reads nothing at its address: the page reads again as it is shown", async () => {
  vi.useRealTimers();
  const release = holdRepoPage("gitea:tidewater/payout-service");
  readLandingAtAddress("#/repo/gitea%3Atidewater%2Fpayout-service");
  await flush();
  expect(sent).toEqual([]);
  release();
  readLandingAtAddress("#/repo/gitea%3Atidewater%2Fpayout-service");
  await flush();
  expect(sent.map((s) => s.cmd)).toContain("get_entity");
});
