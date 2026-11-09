/**
 * The launcher row (#2421): three cells, the monogram coloured by the state's
 * tone, and everything drawn as text.
 *
 * Mounted directly; the cell rule per kind is `cells.ts`'s and is witnessed
 * through the launcher in `Launcher.test.svelte.ts`.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import Row from "./Row.svelte";

function render(over: Record<string, unknown> = {}) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Row, {
    target,
    props: {
      monogram: "PG",
      kindName: "Page",
      cell: "Engineering",
      title: "SEPA payout retry design",
      id: "opt-0",
      selected: false,
      onopen: vi.fn(),
      onhover: vi.fn(),
      ...over,
    },
  });
  flushSync();
  return {
    target,
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a row is the monogram, the key cell and the title, and nothing else", () => {
  const { target, done } = render();
  const row = target.querySelector(".tf-ln-res")!;
  expect([...row.children].map((cell) => cell.textContent)).toEqual(["Pg", "Engineering", "SEPA payout retry design"]);
  done();
});

test("a cell or title that looks like markup is drawn as text", () => {
  const { target, done } = render({ cell: "<b>ENG</b>", title: '<img src=x onerror="alert(1)">' });
  expect(target.textContent).toContain("<b>ENG</b>");
  expect(target.querySelectorAll("b, img")).toHaveLength(0);
  done();
});

test("the monogram carries the tone it is given, and none without one", () => {
  const toned = render({ tone: "ok", kindName: "Pull request: merged" });
  const mg = toned.target.querySelector<HTMLElement>(".tf-ln-mg")!;
  expect(mg.dataset["tone"]).toBe("ok");
  expect(mg.getAttribute("aria-label")).toBe("Pull request: merged");
  toned.done();

  const plain = render();
  expect(plain.target.querySelector(".tf-ln-mg")!.hasAttribute("data-tone")).toBe(false);
  plain.done();
});

test("only an identifier's cell is drawn in the mono face", () => {
  const key = render({ cell: "PAY-231", mono: true });
  expect(key.target.querySelector(".tf-ln-k")!.classList.contains("tf-ln-mono")).toBe(true);
  key.done();
  const place = render();
  expect(place.target.querySelector(".tf-ln-k")!.classList.contains("tf-ln-mono")).toBe(false);
  place.done();
});

test("a forge's issue key is cut to its repository and number, as a card cuts it, whole on hover (the live run, 2026-11-09)", () => {
  const issue = render({ cell: "tidewater/payout-service/issues/37", mono: true, title: "(reserved)" });
  const cell = issue.target.querySelector<HTMLElement>(".tf-ln-k")!;
  expect(cell.textContent).toBe("payout-service#37");
  expect(cell.title).toBe("tidewater/payout-service/issues/37");
  issue.done();
  const key = render({ cell: "PAY-231", mono: true });
  expect(key.target.querySelector<HTMLElement>(".tf-ln-k")!.hasAttribute("title")).toBe(false);
  key.done();
  // A place's cell is words, never a key.
  const place = render({ cell: "tidewater/payout-service/issues/37" });
  expect(place.target.querySelector(".tf-ln-k")!.textContent).toBe("tidewater/payout-service/issues/37");
  place.done();
});
