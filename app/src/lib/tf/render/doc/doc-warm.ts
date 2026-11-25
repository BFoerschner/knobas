/**
 * The XML view warmed while the app is idle (the XML view's first open, 2026-11-25, Fable's ruling), so the first XML
 * or JSON file a reader opens after a start is drawn by warm code and answered by workers already up.
 *
 * Cold, a first open of an order at CPU 6x on 2560 x 1440 at 1.25 paid for the view's code running for the first time
 * (its mount alone some 85 ms), and for each worker's start (the document worker's and the schema worker's first answer
 * some 190 ms after they were made, under 40 at 1x), then for libxml2's load. Two tiers, each once a session, each
 * waiting for the sections the shell builds ahead (`ahead.ts`) and then for the reader to be still and the window idle:
 *
 * - **Tier A, every session.** The view's chunk is loaded, then the view is drawn once off screen, hidden and inert, on
 *   a made-up order and with no schema: the document worker reads its outline, the highlighter colours it. It comes
 *   down once its outline has landed and painted, at the reader's first input, or when a document is opened for real,
 *   whichever is first. Nothing is read and nothing is kept: its folds, its cursor and its check are never asked for.
 * - **Tier B, where this profile checked a document lately** (`checkedLately`, the hint store's): the check's Settings
 *   are read, so the first check starts on them, and the schema worker checks a made-up order against a made-up schema
 *   held in memory, which loads libxml2 and runs the set's code once. No file is read, no set is kept (no pin), and the
 *   session is closed. For a profile that never checks, libxml2 (some 15 MB) stays cold, as D2 and Graphviz do.
 *
 * A document opened before a tier has run cancels it: that open warms what it uses.
 */
import { flushSync, mount, unmount } from "svelte";
import { aheadBuilt, quietAndIdle } from "../../ahead";
import { pageContext } from "../../kept-pages.svelte";
import { docViewModule, onOpenedForReal, openedForReal } from "./doc-module";
import { settingsFor, type CheckSettings } from "./doc-check.svelte";
import { docEngine, type DocEngine } from "../../../render/doc-port";
import { renderPort, type RenderPort } from "../../../render/port";
import { XsdPort, xsdEngine, type XsdEngine } from "../../../render/xsd-port";
import { checkedLately, hintStore } from "../../../render/xsd-hint";
import { xmlCheck, xsdLocations } from "../../../ipc/api";

/** The made-up order: 30 items, 123 lines, one value its schema does not allow. */
export function warmOrder(): string {
  const items = Array.from({ length: 30 }, (_, i) => `  <item n="${i}">\n    <name>Item ${i}</name>\n    <qty>${i === 7 ? -1 : i}</qty>\n  </item>`).join("\n");
  return `<?xml version="1.0"?>\n<order xmlns="urn:knobas:warm">\n${items}\n</order>\n`;
}

/** The made-up schema the order is checked against, held in memory. */
export const WARM_SCHEMA =
  '<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:knobas:warm" xmlns="urn:knobas:warm" elementFormDefault="qualified">' +
  '<xs:element name="order"><xs:complexType><xs:sequence><xs:element name="item" maxOccurs="unbounded"><xs:complexType><xs:sequence>' +
  '<xs:element name="name" type="xs:string"/><xs:element name="qty" type="xs:nonNegativeInteger"/></xs:sequence>' +
  '<xs:attribute name="n" type="xs:int"/></xs:complexType></xs:element></xs:sequence></xs:complexType></xs:element></xs:schema>';

/** The reader's input that takes the hidden view down at once. */
const INPUT = ["keydown", "pointerdown", "wheel", "touchstart"] as const;
/** The longest the hidden view stands, whatever answers. */
const STAND_MS = 4000;

export interface WarmHooks {
  /** What each step waits for: the sections built ahead, then the reader still and the window idle. */
  readonly wait?: () => Promise<void>;
  /** Whether this profile checked a document lately: Tier B runs only then. */
  readonly checks?: () => boolean;
  /** The check's Settings, read for real. */
  readonly settings?: () => Promise<CheckSettings>;
  /** Where the outline is read: the app's document worker. */
  readonly docs?: DocEngine;
  /** Where the set is read and checked: the app's schema worker. */
  readonly schemas?: XsdEngine;
  readonly render?: () => RenderPort;
  /** What takes the hidden view down after its outline landed: two frames, so its paint ran. */
  readonly after?: (go: () => void) => void;
}

let started = false;
/** Takes down a hidden view standing now. */
let takeDown: ((later?: boolean) => void) | null = null;

const waitTurn = async () => {
  await aheadBuilt;
  await quietAndIdle();
};
const twoFrames = (go: () => void) => requestAnimationFrame(() => requestAnimationFrame(go));
const realSettings = () =>
  Promise.all([xsdLocations().then((l) => l.pointed, () => []), xmlCheck().then((c) => c.on, () => true)]).then(([pointed, on]) => ({ pointed, on }));

/** Draws the view once off screen and warms the check where it is used; once a session. Settles when both tiers are done or left. */
export async function warmXmlView(hooks: WarmHooks = {}): Promise<void> {
  if (started || typeof document === "undefined") return;
  if (hooks.wait === undefined && typeof navigator !== "undefined" && /jsdom/i.test(navigator.userAgent)) return;
  started = true;
  const wait = hooks.wait ?? waitTurn;
  // ---- Tier A ----
  await wait();
  if (openedForReal()) return;
  const { default: TfDocView } = await docViewModule();
  await wait();
  if (openedForReal()) return;
  await new Promise<void>((done) => {
    const host = document.createElement("div");
    host.setAttribute("aria-hidden", "true");
    host.inert = true;
    host.className = "tf-warm tf-xv-warm";
    host.style.cssText = "position:fixed;left:-20000px;top:0;width:1200px;height:240px;overflow:hidden;visibility:hidden;pointer-events:none;contain:strict";
    document.body.appendChild(host);
    let view: ReturnType<typeof mount> | null = null;
    let cap: ReturnType<typeof setTimeout> | null = null;
    /** Down: off the page at once; unmounted now, or after the frame of a document opened for real (`later`). */
    const down = (later = false) => {
      if (takeDown !== down) return;
      takeDown = null;
      onOpenedForReal(null);
      if (cap !== null) clearTimeout(cap);
      for (const e of INPUT) removeEventListener(e, stop, { capture: true });
      host.remove();
      const gone = () => {
        try {
          if (view !== null) void unmount(view);
        } catch {
          // A warm-up that fails costs the warmth, never the app.
        } finally {
          done();
        }
      };
      if (later) requestAnimationFrame(() => setTimeout(gone, 0));
      else gone();
    };
    const stop = () => down();
    takeDown = down;
    // Opened for real inside that view's own start: its teardown waits for that view's first frame.
    onOpenedForReal(() => down(true));
    for (const e of INPUT) addEventListener(e, stop, { capture: true, passive: true });
    cap = setTimeout(stop, STAND_MS);
    const docs = hooks.docs ?? docEngine();
    // Down once its outline has landed and painted.
    const engine: DocEngine = (session, call) => {
      const answer = docs(session, call);
      if (call.kind === "open") void answer.then(() => (hooks.after ?? twoFrames)(stop), stop);
      return answer;
    };
    try {
      view = mount(TfDocView, { target: host, props: { text: warmOrder(), path: "knobas-warm/order.xml", repo: "\u0000warm", render: (hooks.render ?? renderPort)(), engine, warming: true }, context: pageContext(() => false) });
      flushSync();
    } catch {
      stop();
    }
  });
  // ---- Tier B ----
  if (openedForReal() || !(hooks.checks ?? (() => checkedLately(hintStore())))()) return;
  await wait();
  if (openedForReal()) return;
  // The Settings the first check starts on, as a kept check does; its verdict is drawn only where they read the same again.
  await settingsFor(hooks.settings ?? realSettings).fresh.catch(() => null);
  const port = new XsdPort(hooks.schemas ?? xsdEngine());
  try {
    const answer = await port.open("knobas-warm/order.xsd", WARM_SCHEMA, { read: async () => null, paths: async () => [], pointed: [], index: async () => ({ declares: [], catalogs: [], total: 0, read: 0 }) }, "urn:knobas:warm", true);
    if (answer.ok) await port.check(warmOrder());
  } catch {
    // As above.
  } finally {
    port.close();
  }
}

/** Tests only: a new session. */
export function forgetXmlWarmth(): void {
  started = false;
  takeDown = null;
}
