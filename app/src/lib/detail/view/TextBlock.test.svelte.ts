/**
 * A text block (ADR-0020): plain text drawn as it is, and Markdown -- a
 * repository's README (#663) -- drawn legibly with real elements, every
 * string still text (rule 5).
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import TextBlock from "./TextBlock.svelte";

function render(text: string, markdown: boolean) {
  const target = document.createElement("div");
  document.body.append(target);
  const onopenurl = vi.fn();
  const app = mount(TextBlock, { target, props: { text, markdown, onopenurl } });
  flushSync();
  return {
    target,
    onopenurl,
    all: (selector: string) => [...target.querySelectorAll<HTMLElement>(selector)],
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

const README = [
  "# payout-service",
  "",
  "Pays out **SEPA** batches. See [the runbook](https://gitea.example/runbook) or [local](file:///etc/passwd).",
  "",
  "## Setup",
  "",
  "- install `just`",
  "- run it",
  "  - nested",
  "",
  "```sh",
  "just check",
  "<img src=x onerror=alert(1)>",
  "```",
  "",
  "<script>alert(1)</script>",
].join("\n");

test("a README draws its headings, lists and code as elements", () => {
  const screen = render(README, true);
  expect(screen.all("h1").map((h) => h.textContent)).toEqual(["payout-service"]);
  expect(screen.all("h2").map((h) => h.textContent)).toEqual(["Setup"]);
  expect(screen.all("li").map((li) => li.textContent?.trim())).toEqual([
    "install just",
    "run it",
    "nested",
  ]);
  expect(screen.all("li code").map((c) => c.textContent)).toEqual(["just"]);
  expect(screen.all("strong").map((s) => s.textContent)).toEqual(["SEPA"]);
  expect(screen.all("pre").map((p) => p.textContent)).toEqual([
    "just check\n<img src=x onerror=alert(1)>",
  ]);
  expect(screen.target.textContent).not.toContain("# payout-service");
  screen.done();
});

test("markup in a README is text, never an element", () => {
  const screen = render(README, true);
  expect(screen.all("script, img")).toHaveLength(0);
  expect(screen.target.textContent).toContain("<script>alert(1)</script>");
  screen.done();
});

test("a web link opens through the shell; any other scheme is only words", () => {
  const screen = render(README, true);
  const links = screen.all("button.md-link");
  expect(links.map((b) => b.textContent)).toEqual(["the runbook"]);
  links[0]!.click();
  expect(screen.onopenurl).toHaveBeenCalledWith("https://gitea.example/runbook");
  expect(screen.target.textContent).toContain("local");
  screen.done();
});

test("plain text stays one paragraph, its hashes and stars as written", () => {
  const screen = render("# not a heading\n*not em*", false);
  expect(screen.all("h1, em, li")).toHaveLength(0);
  expect(screen.all("p").map((p) => p.textContent)).toEqual(["# not a heading\n*not em*"]);
  screen.done();
});
