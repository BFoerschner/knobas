import { afterEach, describe, expect, it } from "vitest";

import { installPlainTyping } from "./plain-typing";

let stop: (() => void) | null = null;
afterEach(() => {
  stop?.();
  stop = null;
  document.body.replaceChildren();
});

function field(html: string): HTMLElement {
  document.body.insertAdjacentHTML("beforeend", html);
  return document.body.lastElementChild as HTMLElement;
}

describe("a one-line box keeps the words as typed", () => {
  it("turns macOS's corrections and capitals off on a line box as it gains focus", () => {
    stop = installPlainTyping();
    const box = field(`<input aria-label="Find in page">`);
    expect(box.hasAttribute("autocorrect")).toBe(false);
    box.focus();
    expect(box.getAttribute("autocorrect")).toBe("off");
    expect(box.getAttribute("autocapitalize")).toBe("off");
    expect(box.spellcheck).toBe(false);
  });

  it("covers search, address and password boxes too", () => {
    stop = installPlainTyping();
    for (const type of ["search", "url", "password", "text"]) {
      const box = field(`<input type="${type}">`);
      box.focus();
      expect(box.getAttribute("autocorrect"), type).toBe("off");
    }
  });

  it("leaves sentences, ticks and a box that chose for itself alone", () => {
    stop = installPlainTyping();
    const area = field(`<textarea></textarea>`);
    const tick = field(`<input type="checkbox">`);
    const chose = field(`<input spellcheck="true">`);
    for (const one of [area, tick, chose]) one.focus();
    expect(area.hasAttribute("autocorrect")).toBe(false);
    expect(tick.hasAttribute("autocorrect")).toBe(false);
    expect(chose.hasAttribute("autocorrect")).toBe(false);
  });

  it("reaches a box that had focus before it was installed", () => {
    const box = field(`<input>`);
    box.focus();
    stop = installPlainTyping();
    expect(box.getAttribute("autocorrect")).toBe("off");
  });
});
