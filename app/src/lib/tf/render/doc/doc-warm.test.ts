/**
 * The XML view warmed at idle (the XML view's first open, 2026-11-25, Fable's ruling): drawn once off screen, hidden and
 * inert, taken down once its outline lands, at the reader's first input or at a real open; the check warmed only where
 * this profile checks, leaving nothing held behind it.
 */
import { afterEach, beforeEach, expect, test } from "vitest";

import { forgetXmlWarmth, warmOrder, warmXmlView } from "./doc-warm";
import { documentOpened, forgetOpened } from "./doc-module";
import { forgetHeld, heldSettings, likelyKept } from "./doc-check.svelte";
import { localDocEngine, type DocEngine } from "../../../render/doc-port";
import type { RenderPort } from "../../../render/port";
import { answerXsd, type XsdSessions } from "../../../render/xsd-answer";
import { keptOf } from "../../../render/xsd-kept";
import type { XsdEngine } from "../../../render/xsd-port";
import { bundledFiles } from "../../../render/xsd-fixture";

const quiet: RenderPort = { highlight: () => ({ focus: () => {}, cancel: () => {} }) };
const b = bundledFiles();
const BUNDLED = { byNs: b.byNs, texts: b.texts };
const flush = () => new Promise((r) => setTimeout(r, 0));

/** A schema worker on this thread, and what it was asked. */
function schemas() {
  const sessions: XsdSessions = new Map();
  const asked: string[] = [];
  const engine: XsdEngine = async (id, call) => (asked.push(call.kind), answerXsd(sessions, id, call, BUNDLED));
  return { sessions, asked, engine };
}

/** Waits the test hands over by hand, one a step. */
function steps() {
  const queue: (() => void)[] = [];
  return {
    wait: () => new Promise<void>((go) => queue.push(go)),
    async next() {
      // The view's chunk loads between two steps: a while, the first time.
      for (let i = 0; i < 1000 && queue.length === 0; i += 1) await new Promise((r) => setTimeout(r, 10));
      queue.shift()?.();
      await flush();
    },
    get waiting() {
      return queue.length;
    },
  };
}

beforeEach(() => {
  forgetXmlWarmth();
  forgetOpened();
  forgetHeld();
});
afterEach(() => document.querySelectorAll(".tf-warm").forEach((n) => n.remove()));

test("the view is drawn off screen, hidden and inert, on the made-up order, and taken down once its outline lands; once a session", async () => {
  const s = steps();
  let opened = 0;
  const docs: DocEngine = (id, call) => {
    if (call.kind === "open") opened += 1;
    return localDocEngine()(id, call);
  };
  let paint: (() => void) | null = null;
  const done = warmXmlView({ wait: s.wait, checks: () => false, docs, render: () => quiet, after: (go) => (paint = go) });
  await s.next();
  await s.next();
  const host = document.querySelector<HTMLElement>(".tf-warm");
  expect(host).not.toBeNull();
  expect(host!.getAttribute("aria-hidden")).toBe("true");
  expect(host!.inert).toBe(true);
  expect(host!.style.visibility).toBe("hidden");
  expect(host!.querySelector(".tf-xv .tf-l")).not.toBeNull();
  for (let i = 0; i < 500 && paint === null; i += 1) await new Promise((r) => setTimeout(r, 10));
  expect(opened, "its outline asked once").toBe(1);
  expect(document.querySelector(".tf-warm"), "standing until its outline has painted").not.toBeNull();
  paint!();
  await done;
  expect(document.querySelector(".tf-warm"), "nothing left behind").toBeNull();
  let again = 0;
  await warmXmlView({ wait: async () => void (again += 1) });
  expect(again, "once a session").toBe(0);
});

test("the reader's first input takes the hidden view down at once", async () => {
  const s = steps();
  const done = warmXmlView({ wait: s.wait, checks: () => false, docs: () => new Promise(() => {}), render: () => quiet });
  await s.next();
  await s.next();
  expect(document.querySelector(".tf-warm")).not.toBeNull();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "j", bubbles: true }));
  expect(document.querySelector(".tf-warm"), "at once, not when its time is up").toBeNull();
  await done;
});

test("a document opened for real takes the hidden view down, and one opened before cancels what is still to warm", async () => {
  const s = steps();
  const x = schemas();
  const done = warmXmlView({ wait: s.wait, checks: () => true, docs: () => new Promise(() => {}), render: () => quiet, schemas: x.engine, settings: async () => ({ on: true, pointed: [] }) });
  await s.next();
  await s.next();
  expect(document.querySelector(".tf-warm")).not.toBeNull();
  let gone = false;
  void done.then(() => (gone = true));
  documentOpened();
  expect(document.querySelector(".tf-warm"), "off the page at once, not when its time is up").toBeNull();
  await flush();
  expect(gone, "unmounted after that view's first frame, not inside its start").toBe(false);
  await done;
  // Tier B was still to come: left.
  expect(x.asked).toEqual([]);
  expect(heldSettings()).toBeNull();

  forgetXmlWarmth();
  const t = steps();
  const before = warmXmlView({ wait: t.wait, checks: () => true, docs: () => new Promise(() => {}), render: () => quiet, schemas: x.engine });
  await t.next();
  await before;
  expect(document.querySelector(".tf-warm"), "nothing drawn").toBeNull();
});

test("the check warms only where this profile checks: libxml2's verdict on the made-up order, nothing kept, the real Settings held", async () => {
  for (const checks of [false, true]) {
    forgetXmlWarmth();
    forgetHeld();
    const x = schemas();
    let paint: (() => void) | null = null;
    const s = steps();
    const done = warmXmlView({ wait: s.wait, checks: () => checks, docs: localDocEngine(), render: () => quiet, after: (go) => (paint = go), schemas: x.engine, settings: async () => ({ on: true, pointed: [{ namespace: "urn:p", location: "p.xsd", to: { repo: "r", path: "p.xsd" } }] }) });
    await s.next();
    await s.next();
    for (let i = 0; i < 500 && paint === null; i += 1) await new Promise((r) => setTimeout(r, 10));
    paint!();
    if (checks) await s.next();
    await done;
    if (!checks) {
      expect(x.asked, "no schema worker asked").toEqual([]);
      expect(heldSettings()).toBeNull();
      continue;
    }
    expect(x.asked).toEqual(["open", "check", "close"]);
    // Nothing held behind it: no session, no kept set, no ask seen whole.
    expect(x.sessions.size).toBe(0);
    expect(keptOf(x.sessions).size).toBe(0);
    expect(likelyKept("knobas-warm/order.xml", { name: "order", namespace: "urn:knobas:warm", location: null }, { scope: "r", at: "c" }, [])).toBe(false);
    // The Settings the first check starts on are the real ones.
    expect(heldSettings()).toEqual({ on: true, pointed: [{ namespace: "urn:p", location: "p.xsd", to: { repo: "r", path: "p.xsd" } }] });
  }
});

test("the made-up order is checked by libxml2 against the made-up schema: one departure", async () => {
  const x = schemas();
  const sessions = x.sessions;
  const { WARM_SCHEMA } = await import("./doc-warm");
  answerXsd(sessions, 1, { kind: "open", entry: "knobas-warm/order.xsd", text: WARM_SCHEMA, paths: null, pointed: [], lean: true }, BUNDLED);
  const c = (await answerXsd(sessions, 1, { kind: "check", xml: warmOrder() }, BUNDLED)) as { state: string; total: number };
  expect(c.state).toBe("checked");
  expect(c.total).toBe(1);
  expect(warmOrder().split("\n").length).toBeGreaterThan(120);
});
