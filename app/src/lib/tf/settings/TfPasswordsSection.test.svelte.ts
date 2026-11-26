/**
 * A password source's Passwords section (ADR-0056): where a source marks no
 * field protected it says every value is mirrored and searched (Fable's
 * ruling while Björn was away, 2026-11-22); the code keys' table starts with
 * the server's own field and counts each format the sync found.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { Offers } from "../../ipc/passwords";
import TfPasswordsSection from "./TfPasswordsSection.svelte";

const OFFERS: Offers = {
  one_time_codes: true, expiry: true, usage_comments: true, audit_log: true, history: null, attachments: true, access_requests: false,
  marks_protected_fields: false, writes: ["set_password"],
};

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

async function open(offers: Offers): Promise<void> {
  app = mount(TfPasswordsSection, {
    target,
    props: { sourceId: "pw", name: "Team passwords", account: "mara", offers, synced: true, facts: async () => ({ entries: 20, folders: 31, formats: [["pleasant", 2], ["tray_totp", 1]] }) },
  });
  for (let i = 0; i < 4; i++) await Promise.resolve();
  flushSync();
}

test("a source that marks no field protected says every value rests in the mirror and is searched", async () => {
  await open(OFFERS);
  expect(target.querySelector(".tf-pw-fieldsnote")?.textContent?.replace(/\s+/g, " ")).toContain("so knobas cannot tell a PIN from a port number");
  unmount(app!);
  app = null;
  await open({ ...OFFERS, marks_protected_fields: true });
  expect(target.querySelector(".tf-pw-fieldsnote")).toBeNull();
});

test("the code keys' table starts with the server's own field and counts what the sync found", async () => {
  await open(OFFERS);
  const rows = [...target.querySelectorAll(".tf-pw-formats tbody tr")].map((tr) => [...tr.querySelectorAll("td")].map((td) => td.textContent?.trim()));
  expect(rows[0]?.slice(0, 2)).toEqual(["The server's own", "TOTPSecret"]);
  expect(target.querySelectorAll(".tf-pw-formats tr.tf-pw-found")).toHaveLength(2);
});
