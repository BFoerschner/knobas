/**
 * The entity-list block (ADR-0020 rule 7), as a pull request's *Commits* tab
 * draws it (#662): a commit the mirror holds is an address that opens its own
 * detail; one it does not hold is plain text with the source's page. The short
 * sha is a chip, the author and age the quiet reading after it.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import type { ViewRow } from "../../ipc/entity";
import { parseHash } from "../../shell/router.svelte";
import EntitiesBlock from "./EntitiesBlock.svelte";

const HELD = "gitea:tidewater/payout-service@7be0e4c1d2e3f405162738495a6b7c8d9e0f1a2b";

const ROWS: ViewRow[] = [
  {
    entity_id: HELD,
    title: "PAY-231: retry SEPA payouts on transient PSP errors",
    meta: "Mara Lindqvist · 2026-08-21",
    tone: null,
    chip: "7be0e4c1d2",
    url: "https://gitea.example/tidewater/payout-service/commit/7be0e4c1d2",
  },
  {
    // The app cleared the address: the mirror does not hold this commit.
    entity_id: null,
    title: "PAY-231: jitter <b>in</b> backoff",
    meta: "Mara Lindqvist · 3 h ago",
    tone: null,
    chip: "c90d11a3f5",
    url: "https://gitea.example/tidewater/payout-service/commit/c90d11a3f5",
  },
];

function render(rows: ViewRow[]) {
  const target = document.createElement("div");
  document.body.append(target);
  const navigated: string[] = [];
  const opened: string[] = [];
  const app = mount(EntitiesBlock, {
    target,
    props: {
      title: null,
      rows,
      onnavigate: (hash: string) => navigated.push(hash),
      onopenurl: (url: string) => opened.push(url),
    },
  });
  flushSync();
  return {
    target,
    navigated,
    opened,
    rows: () => [...target.querySelectorAll<HTMLElement>(".eb-row")],
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a commit the mirror holds opens its own detail", () => {
  const screen = render(ROWS);
  const link = screen.rows()[0]!.querySelector<HTMLButtonElement>("button.eb-t")!;
  expect(link.textContent!.trim()).toBe("PAY-231: retry SEPA payouts on transient PSP errors");
  link.click();
  expect(screen.navigated).toHaveLength(1);
  expect(parseHash(screen.navigated[0]!)).toMatchObject({
    detail: { kind: null, entityId: HELD },
  });
  // An address needs no second way out to the source.
  expect(screen.rows()[0]!.querySelector(".btn")).toBeNull();
  screen.done();
});

test("a commit the mirror does not hold is text, with the source's page", () => {
  const screen = render(ROWS);
  const row = screen.rows()[1]!;
  expect(row.querySelector("button.eb-t")).toBeNull();
  const title = row.querySelector(".eb-t.plain")!;
  expect(title.textContent).toBe("PAY-231: jitter <b>in</b> backoff");
  expect(title.querySelector("b")).toBeNull();
  row.querySelector<HTMLButtonElement>(".btn")!.click();
  expect(screen.opened).toEqual([
    "https://gitea.example/tidewater/payout-service/commit/c90d11a3f5",
  ]);
  expect(screen.navigated).toEqual([]);
  screen.done();
});

test("the short sha is a chip and the author and age follow it", () => {
  const screen = render(ROWS);
  const read = screen.rows().map((row) => [
    row.querySelector(".st")?.textContent?.trim(),
    row.querySelector(".eb-meta")?.textContent,
  ]);
  expect(read).toEqual([
    ["7be0e4c1d2", "Mara Lindqvist · 2026-08-21"],
    ["c90d11a3f5", "Mara Lindqvist · 3 h ago"],
  ]);
  screen.done();
});

test("a list's note that it was cut reads muted, with the source's page", () => {
  const note: ViewRow = {
    entity_id: null,
    title: "The first 250 of 300 commits; the rest are on Gitea.",
    meta: null,
    tone: "muted",
    chip: null,
    url: "https://gitea.example/tidewater/payout-service/pulls/142/commits",
  };
  const screen = render([...ROWS, note]);
  const [held, unheld, cut] = screen.rows().map((row) => row.querySelector(".eb-t")!);
  expect(cut!.classList.contains("muted")).toBe(true);
  // A row whose chip carries the tone keeps its title plain.
  expect(held!.classList.contains("muted")).toBe(false);
  expect(unheld!.classList.contains("muted")).toBe(false);
  screen.rows()[2]!.querySelector<HTMLButtonElement>(".btn")!.click();
  expect(screen.opened).toEqual([note.url]);
  screen.done();
});
