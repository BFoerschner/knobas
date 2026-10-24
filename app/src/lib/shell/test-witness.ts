import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * **Where a desktop-witness driver expects a step to land** (#1968), read out
 * of the driver itself, so a DOM test can hold the app to it.
 *
 * The drivers under `testenv/desktop-witness/drivers/` take the screen, so no
 * gate runs them, and an app change that moves where a step lands left them
 * asserting the old page: `capture.sh` waited for Settings › Notes and capture
 * after *Open in knobas* for months after #1504 sent the note to its own page.
 * A driver now waits on a landing with `lands <landing> "$READING"...` (whole
 * lines `ax values` reads) or `lands_named <landing> "$NAME"...` (accessible
 * names `ax find` counts), each argument one of the driver's `readonly`
 * constants. A test replays the step in the DOM and asserts what this returns;
 * `desktop-witness-test.sh` refuses a landing no test replays.
 */
export interface Landing {
  /** Whole lines of text the page draws (`lands`). */
  reads: string[];
  /** Accessible names the page carries (`lands_named`). */
  names: string[];
}

const DRIVERS = join(process.cwd(), "../testenv/desktop-witness/drivers");

/** The driver's source, by its name under `drivers/` (`capture` is `capture.sh`). */
export function driverSource(driver: string): string {
  return readFileSync(join(DRIVERS, `${driver}.sh`), "utf8");
}

/** `readonly NAME='value'` (or `"value"` with nothing to expand) at the start of a line. */
function constantsOf(source: string): Map<string, string> {
  const constants = new Map<string, string>();
  for (const match of source.matchAll(/^readonly ([A-Z][A-Z0-9_]*)=(?:'([^']*)'|"([^"$`\\]*)")/gm)) {
    constants.set(match[1]!, match[2] ?? match[3]!);
  }
  return constants;
}

/**
 * The landing `landing` of a driver's source: every `lands`/`lands_named` call
 * naming it, its arguments resolved. Throws on a call this cannot read (an
 * argument that is not a quoted constant, a constant the driver lacks), on two
 * calls of one kind that disagree, and on a landing the driver never names.
 */
export function landingOf(source: string, landing: string): Landing {
  const constants = constantsOf(source);
  const found: { reads: string[] | null; names: string[] | null } = { reads: null, names: null };
  for (const line of source.split("\n")) {
    const call = /^\s*(lands|lands_named)\s+(.*)$/.exec(line);
    if (call === null) continue;
    const [, verb, rest] = call;
    const words = rest!.trim().split(/\s+/);
    if (words[0] !== landing) continue;
    const values = words.slice(1).map((word) => {
      const name = /^"\$([A-Z][A-Z0-9_]*)"$/.exec(word)?.[1];
      if (name === undefined) throw new Error(`${landing}: ${word} is not a quoted constant ("$NAME") in: ${line.trim()}`);
      const value = constants.get(name);
      if (value === undefined) throw new Error(`${landing}: the driver has no readonly ${name}`);
      return value;
    });
    if (values.length === 0) throw new Error(`${landing}: ${verb} names nothing to wait on`);
    const kind = verb === "lands" ? "reads" : "names";
    const earlier = found[kind];
    if (earlier !== null && earlier.join("\n") !== values.join("\n")) {
      throw new Error(`${landing}: two ${verb} calls disagree (${earlier.join(", ")} / ${values.join(", ")})`);
    }
    found[kind] = values;
  }
  if (found.reads === null && found.names === null) throw new Error(`the driver never lands on ${landing}`);
  return { reads: found.reads ?? [], names: found.names ?? [] };
}

/** `landingOf` the driver named `driver`. */
export function landing(driver: string, name: string): Landing {
  return landingOf(driverSource(driver), name);
}

/** The text an element draws itself, its children's elements left out: what one AXStaticText reads. */
function ownText(element: Element): string {
  let text = "";
  for (const node of element.childNodes) if (node.nodeType === Node.TEXT_NODE) text += node.textContent;
  return text.trim();
}

/** Whether a line of text under `root` reads exactly `reading`, as `has_reading` compares. */
export function drawsReading(root: Element, reading: string): boolean {
  return [root, ...root.querySelectorAll("*")].some((element) => ownText(element) === reading);
}

/**
 * Whether an element under `root` answers to `name` as `ax find` counts it:
 * an `aria-label`, a placeholder, or a button's or a link's own text (its
 * AXTitle).
 */
export function carriesName(root: Element, name: string): boolean {
  return [root, ...root.querySelectorAll("*")].some(
    (element) =>
      element.getAttribute("aria-label") === name ||
      element.getAttribute("placeholder") === name ||
      ((element.tagName === "BUTTON" || element.tagName === "A") && element.textContent?.trim() === name),
  );
}

/** What of a landing `root` does not draw, `[]` when it lands. */
export function missingFrom(root: Element, expected: Landing): string[] {
  return [
    ...expected.reads.filter((reading) => !drawsReading(root, reading)).map((reading) => `the line '${reading}'`),
    ...expected.names.filter((name) => !carriesName(root, name)).map((name) => `the name '${name}'`),
  ];
}
