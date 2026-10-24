import { describe, expect, it } from "vitest";
import { carriesName, drawsReading, landingOf, missingFrom } from "./test-witness";

/** A driver in the shape the real ones take: constants, comments, a wait. */
const DRIVER = `readonly NOTE_LINE='A note, read-only in this release.'  # note/TfNote.svelte
readonly CAPTURED_FROM='Captured from'
readonly OPEN_CODE_VIEW="Open Code view"
# lands open-in-knobas "$NOT_A_CONSTANT" -- a comment, never read
lands open-in-knobas "$NOTE_LINE" "$CAPTURED_FROM"
    lands_named job-link "$OPEN_CODE_VIEW"
lands open-in-knobas "$NOTE_LINE" "$CAPTURED_FROM"
`;

describe("landingOf (#1968)", () => {
  it("resolves each argument to the driver's constant, by kind", () => {
    expect(landingOf(DRIVER, "open-in-knobas")).toEqual({
      reads: ["A note, read-only in this release.", "Captured from"],
      names: [],
    });
    expect(landingOf(DRIVER, "job-link")).toEqual({ reads: [], names: ["Open Code view"] });
  });

  it("refuses a landing the driver never names", () => {
    expect(() => landingOf(DRIVER, "settings-notes")).toThrow("never lands on settings-notes");
  });

  it("refuses an argument that is not a quoted constant", () => {
    expect(() => landingOf(`readonly A='a'\nlands here "$A" "$title"\n`, "here")).toThrow("not a quoted constant");
    expect(() => landingOf(`lands here 'Your notes'\n`, "here")).toThrow("not a quoted constant");
  });

  it("refuses a constant the driver lacks", () => {
    expect(() => landingOf(`lands here "$GONE"\n`, "here")).toThrow("no readonly GONE");
  });

  it("refuses two calls of one landing that wait on different lines", () => {
    expect(() => landingOf(`readonly A='a'\nreadonly B='b'\nlands here "$A"\nlands here "$B"\n`, "here")).toThrow(
      "disagree",
    );
  });
});

describe("the DOM readings (#1968)", () => {
  function page(html: string): HTMLElement {
    const root = document.createElement("div");
    root.innerHTML = html;
    return root;
  }

  it("reads a whole line an element draws itself, not a longer one or a child's", () => {
    const root = page(`<span>Captured from here</span><div>Cloned at <span>/tmp/x</span></div>`);
    expect(drawsReading(root, "Captured from")).toBe(false);
    expect(drawsReading(root, "Cloned at")).toBe(true);
    expect(drawsReading(root, "/tmp/x")).toBe(true);
    expect(drawsReading(root, "Cloned at /tmp/x")).toBe(false);
  });

  it("names an element by its aria-label, its placeholder, or a button's or link's text", () => {
    const root = page(
      `<div aria-label="Show notes"></div><input placeholder="Filter"><a href="#/code">Open Code view</a><span>Plain</span>`,
    );
    expect(carriesName(root, "Show notes")).toBe(true);
    expect(carriesName(root, "Filter")).toBe(true);
    expect(carriesName(root, "Open Code view")).toBe(true);
    expect(carriesName(root, "Plain")).toBe(false);
  });

  it("lists what a page does not draw of a landing", () => {
    const root = page(`<h1>Retry storm</h1>`);
    expect(missingFrom(root, { reads: ["Retry storm", "Your notes"], names: ["Settings"] })).toEqual([
      "the line 'Your notes'",
      "the name 'Settings'",
    ]);
  });
});
