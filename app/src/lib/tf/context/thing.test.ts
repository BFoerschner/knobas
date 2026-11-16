/** What a right-click is on (`thing.ts`): read from the page as drawn, nothing from the backend. */
import { afterEach, expect, test } from "vitest";

import { isEditable, menuThing, selectionAt, thingAt, thingOfHash } from "./thing";

const ORIGIN = "http://app.test";

afterEach(() => {
  document.body.replaceChildren();
});

function page(html: string): HTMLElement {
  const root = document.createElement("div");
  root.innerHTML = html;
  document.body.append(root);
  return root;
}

test("an address is the thing it opens; a place is no thing", () => {
  expect(thingOfHash("#/ticket/jira%3APAY-231")).toEqual({ kind: "ticket", id: "jira:PAY-231", hash: "#/ticket/jira%3APAY-231" });
  expect(thingOfHash("#/job/jira%3APAY-231/code")).toEqual({ kind: "ticket", id: "jira:PAY-231", hash: "#/ticket/jira%3APAY-231" });
  expect(thingOfHash("#/pr/gitea%3Ax%2Fy%2312/diff")?.kind).toBe("pr");
  expect(thingOfHash("#/repo/gitea%3Ax%2Fy/branches")).toEqual({ kind: "repo", id: "gitea:x/y", hash: "#/repo/gitea%3Ax%2Fy" });
  expect(thingOfHash("#/repo/r/files/src/lib.rs")).toEqual({ kind: "file", repoId: "r", path: "src/lib.rs" });
  expect(thingOfHash("#/repo/r/line/42/src/lib.rs")).toEqual({ kind: "line", repoId: "r", path: "src/lib.rs", line: 42 });
  expect(thingOfHash("#/repo/r/commits/c%40abc")).toEqual({ kind: "commit", id: "c@abc", repoId: "r", hash: "#/repo/r/commits/c%40abc" });
  expect(thingOfHash("#/commit/c%40abc")).toEqual({ kind: "commit", id: "c@abc", repoId: null, hash: "#/commit/c%40abc" });
  expect(thingOfHash("#/build/p/r1/log")).toEqual({ kind: "run", id: "r1", pipelineId: "p", hash: "#/build/p/r1" });
  expect(thingOfHash("#/build/r1")).toEqual({ kind: "run", id: "r1", pipelineId: null, hash: "#/build/r1" });
  expect(thingOfHash("#/builds/p")?.kind).toBe("pipeline");
  expect(thingOfHash("#/page/c%3A9")?.kind).toBe("page");
  expect(thingOfHash("#/monitors/at/k%3A3")).toEqual({ kind: "monitor", id: "k:3", hash: "#/monitors/at/k%3A3" });
  for (const place of ["#/tickets", "#/code", "#/code/prs", "#/builds", "#/monitors", "#/monitors/unmonitored", "#/settings/checkouts", "#/inbox", "#/nowhere"]) {
    expect(thingOfHash(place), place).toBeNull();
  }
});

test("the nearest link or tag up from the press wins, so a button inside a row gives the row's", () => {
  const root = page(`<table><tr class="row"><td><a href="#/pr/p1">Fix</a></td><td><button class="act">Ack</button></td></tr></table>`);
  const row = root.querySelector(".row")!;
  const off = menuThing(() => ({ kind: "monitor", id: "m1", hash: "#/monitors/at/m1", alerting: true }))(row);
  expect(thingAt(root.querySelector(".act"), "#/monitors", ORIGIN)?.thing).toEqual({ kind: "monitor", id: "m1", hash: "#/monitors/at/m1", alerting: true });
  expect(thingAt(root.querySelector("a"), "#/monitors", ORIGIN)?.thing.kind).toBe("pr");
  off();
  // Untagged, a table's row is the thing its first link names.
  expect(thingAt(root.querySelector(".act"), "#/monitors", ORIGIN)).toMatchObject({ thing: { kind: "pr", id: "p1" }, node: root.querySelector("a") });
});

test("a link to a place is passed over for the row around it; a web link is a web thing", () => {
  const root = page(`<div class="r"><a class="tab" href="#/code/prs">PRs</a> <a class="out" href="https://gitea.example/x">x</a> <a class="self" href="${ORIGIN}/#/x">s</a></div>`);
  menuThing(() => ({ kind: "test", name: "retries" }))(root.querySelector(".r")!);
  expect(thingAt(root.querySelector(".tab"), "", ORIGIN)?.thing).toEqual({ kind: "test", name: "retries" });
  expect(thingAt(root.querySelector(".out"), "", ORIGIN)?.thing).toEqual({ kind: "web", url: "https://gitea.example/x" });
  expect(thingAt(root.querySelector(".self"), "", ORIGIN)?.thing).toEqual({ kind: "test", name: "retries" });
});

test("on a repository's page: a tree folder, a line of the file view and a repository image are its own", () => {
  const root = page(
    `<button class="tf-fdir" data-path="src/sepa">sepa</button>` +
      `<div class="tf-fileview"><pre><div class="tf-l" data-n="7"><span>7</span><span class="t">fn x</span></div></pre></div>` +
      `<img data-md-path="/docs/a.png" alt="">`,
  );
  const hash = "#/repo/r/files/src/lib.rs";
  expect(thingAt(root.querySelector(".tf-fdir"), hash, ORIGIN)?.thing).toEqual({ kind: "folder", repoId: "r", path: "src/sepa" });
  expect(thingAt(root.querySelector(".t"), hash, ORIGIN)?.thing).toEqual({ kind: "line", repoId: "r", path: "src/lib.rs", line: 7 });
  expect(thingAt(root.querySelector("img"), hash, ORIGIN)?.thing).toEqual({ kind: "file", repoId: "r", path: "docs/a.png" });
  // Not on a repository's page, they are nothing.
  expect(thingAt(root.querySelector(".tf-fdir"), "#/inbox", ORIGIN)).toBeNull();
});

test("a page's heading is the page's own thing", () => {
  const root = page(`<main class="tf-main"><header><h1><span class="w">Payout CSV</span></h1></header><p class="body">x</p></main>`);
  expect(thingAt(root.querySelector(".w"), "#/ticket/jira%3APAY-1", ORIGIN)).toMatchObject({ own: true, thing: { kind: "ticket", id: "jira:PAY-1" } });
  expect(thingAt(root.querySelector(".body"), "#/ticket/jira%3APAY-1", ORIGIN)).toBeNull();
  expect(thingAt(root.querySelector(".w"), "#/inbox", ORIGIN)).toBeNull();
});

test("a field a person types in is the platform's; a checkbox is not", () => {
  const root = page(`<input class="t"><input type="checkbox" class="c"><textarea></textarea><div contenteditable="true"><b class="e">x</b></div><select></select>`);
  expect(isEditable(root.querySelector(".t"))).toBe(true);
  expect(isEditable(root.querySelector("textarea"))).toBe(true);
  expect(isEditable(root.querySelector(".e"))).toBe(true);
  expect(isEditable(root.querySelector("select"))).toBe(true);
  expect(isEditable(root.querySelector(".c"))).toBe(false);
  expect(isEditable(root)).toBe(false);
});

test("the selection counts only when the press is inside it", () => {
  const root = page(`<p class="a">alpha beta</p><p class="b">gamma</p>`);
  const range = document.createRange();
  range.selectNodeContents(root.querySelector(".a")!);
  const selection = getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  expect(selectionAt(root.querySelector(".a"), selection)).toBe("alpha beta");
  expect(selectionAt(root.querySelector(".b"), selection)).toBeNull();
  selection.removeAllRanges();
  expect(selectionAt(root.querySelector(".a"), selection)).toBeNull();
});
