import { afterEach, expect, test, vi } from "vitest";

import { capturePlace, resetScrolls, restorePlace } from "./return-place";

/** jsdom lays nothing out, so a scroller's offset is just a number it holds. */
function draw(html: string): { main: HTMLElement; page: HTMLElement } {
  document.body.innerHTML = `<main class="tf-main"><div class="tf-kept">${html}</div></main>`;
  return { main: document.querySelector<HTMLElement>(".tf-main")!, page: document.querySelector<HTMLElement>(".tf-kept")! };
}

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const frames = () => new Promise((resolve) => setTimeout(resolve, 60));

test("the page scroller and an inner scroller come back to where they were left, an inner one at 0 too", () => {
  const { main, page } = draw(`<div class="tf-scroller tf-bmain"><table><tr><td>x</td></tr></table></div>`);
  const inner = page.querySelector(".tf-bmain")!;
  main.scrollTop = 60;
  const place = capturePlace(main, page, null, "#/pr/1");
  main.scrollTop = 300;
  inner.scrollTop = 200;
  const end = restorePlace(place, page);
  end();
  expect(main.scrollTop).toBe(60);
  expect(inner.scrollTop).toBe(0);
});

test("a scroller redrawn while away is found again by its classes", () => {
  const { main, page } = draw(`<div class="tf-board-wrap"></div>`);
  const wrap = page.querySelector(".tf-board-wrap")!;
  wrap.scrollTop = 120;
  wrap.scrollLeft = 200;
  const place = capturePlace(main, page, null, "#/ticket/x");
  wrap.remove();
  page.insertAdjacentHTML("beforeend", `<div class="tf-board-wrap"></div>`);
  restorePlace(place, page)();
  const again = page.querySelector(".tf-board-wrap")!;
  expect([again.scrollTop, again.scrollLeft]).toEqual([120, 200]);
});

test("focus goes to the row's link to where it opened, not to the part pressed", () => {
  const { main, page } = draw(`<table><tr class="tf-row"><td id="cell">name</td><td><a id="other" href="#/repo/9">repo</a></td><td><a id="own" href="#/pr/1">PR</a></td></tr></table>`);
  const place = capturePlace(main, page, page.querySelector("#cell"), "#/pr/1");
  restorePlace(place, page)();
  expect(document.activeElement?.id).toBe("own");
});

test("a row redrawn while away is found by the address it opened", () => {
  const { main, page } = draw(`<ul><li><a id="old" href="#/pr/1">PR</a></li></ul>`);
  const place = capturePlace(main, page, page.querySelector("#old"), "#/pr/1");
  page.innerHTML = `<ul><li><a id="new" href="#/pr/1">PR</a></li></ul>`;
  restorePlace(place, page)();
  expect(document.activeElement?.id).toBe("new");
});

test("a ticket's card takes focus by its entity, whichever control opened it", () => {
  const { main, page } = draw(`<article data-entity="mock:PAY-231" tabindex="0" id="card"></article><a id="open" href="#/ticket/mock%3APAY-231">Open</a>`);
  const place = capturePlace(main, page, page.querySelector("#open"), "#/ticket/mock%3APAY-231");
  restorePlace(place, page)();
  expect(document.activeElement?.id).toBe("card");
});

test("focus a dialog or a field already took stays there", () => {
  const { main, page } = draw(`<a id="row" href="#/pr/1">PR</a><input id="field" />`);
  const place = capturePlace(main, page, page.querySelector("#row"), "#/pr/1");
  page.querySelector<HTMLElement>("#field")!.focus();
  restorePlace(place, page)();
  expect(document.activeElement?.id).toBe("field");
});

test("the reader moving ends the hold: their scroll stands", async () => {
  const { main, page } = draw(`<a href="#/pr/1">PR</a>`);
  main.scrollTop = 60;
  const place = capturePlace(main, page, null, "#/pr/1");
  const end = restorePlace(place, page);
  window.dispatchEvent(new Event("wheel"));
  main.scrollTop = 500;
  await frames();
  expect(main.scrollTop).toBe(500);
  end();
});

test("a redraw during the hold is put right", async () => {
  const { main, page } = draw(`<a href="#/pr/1">PR</a>`);
  main.scrollTop = 60;
  const place = capturePlace(main, page, null, "#/pr/1");
  const end = restorePlace(place, page);
  main.scrollTop = 500;
  await frames();
  expect(main.scrollTop).toBe(60);
  end();
});

test("a new place in a kept page starts at the top of each scroller", () => {
  const { main, page } = draw(`<div class="tf-scroller tf-bmain"></div>`);
  const inner = page.querySelector(".tf-bmain")!;
  main.scrollTop = 40;
  inner.scrollTop = 300;
  resetScrolls(main, page);
  expect([main.scrollTop, inner.scrollTop]).toEqual([0, 0]);
});
