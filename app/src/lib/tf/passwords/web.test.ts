import { expect, test } from "vitest";

import type { PwSource } from "../../ipc/passwords";
import type { SiteSetting } from "../../ipc/site";
import { fillsAs, opensInside, webOpens, webWords } from "./web";

const SITE: SiteSetting = {
  source: "pw", open_inside: true, base: "https://pw.example", own: "https://pw.example", allowed: [], suggested: [],
  kept_since: null, offered: true, persists: true, sign_in_user: null, own_sign_in: null, confined: true,
};

function source(over: Partial<PwSource> = {}): PwSource {
  return {
    id: "pw", name: "Team passwords", account: "mara", by_hand: true, synced_at: null, down_at: null, never_synced: false,
    offers: { one_time_codes: false, expiry: false, usage_comments: false, audit_log: false, history: null, attachments: false, access_requests: false, marks_protected_fields: false, writes: [] },
    skipped_root: null, adapter_name: "Any", web: { entry: true, folder: true },
    ...over,
  };
}

test("a press opens the entry's page, else its folder's, else the front page", () => {
  expect(webOpens("entry", { entry: true, folder: true }, true)).toBe("entry");
  expect(webOpens("entry", { entry: false, folder: true }, true)).toBe("entry_folder");
  expect(webOpens("entry", { entry: false, folder: true }, false)).toBe("home");
  expect(webOpens("entry", { entry: false, folder: false }, true)).toBe("home");
  expect(webOpens("folder", { entry: true, folder: false }, true)).toBe("home");
  expect(webOpens("folder", { entry: false, folder: true }, true)).toBe("folder");
  expect(webOpens("home", { entry: true, folder: true }, true)).toBe("home");
});

test("inside needs the toggle on and sites offered; the sign-in is the saved one, else the source's own when on", () => {
  expect(opensInside(SITE)).toBe(true);
  expect(opensInside({ ...SITE, open_inside: false })).toBe(false);
  expect(opensInside({ ...SITE, offered: false })).toBe(false);
  expect(opensInside(undefined)).toBe(false);
  expect(fillsAs(SITE)).toBeNull();
  expect(fillsAs({ ...SITE, own_sign_in: { user: "mara", on: true } })).toBe("mara");
  expect(fillsAs({ ...SITE, own_sign_in: { user: "mara", on: false } })).toBeNull();
  expect(fillsAs({ ...SITE, sign_in_user: "admin", own_sign_in: { user: "mara", on: false } })).toBe("admin");
});

test("the words follow where it opens, who signs in and whether the source keeps a log", () => {
  expect(webWords(source(), "entry", true, SITE)?.title).toBe(
    "Opens this entry in the password manager's web interface, inside knobas. Shift W opens it in your browser instead.",
  );
  const logged = source({ offers: { ...source().offers, audit_log: true }, web: { entry: false, folder: true } });
  expect(webWords(logged, "entry", true, { ...SITE, own_sign_in: { user: "mara", on: true } })?.title).toBe(
    "Opens this entry's folder in the password manager's web interface, inside knobas and signed in. A new sign-in is recorded there under mara. Shift W opens it in your browser instead.",
  );
  expect(webWords(logged, "folder", false, { ...SITE, open_inside: false, own_sign_in: { user: "mara", on: true } })?.title).toBe(
    "Opens this folder in the password manager's web interface, in your browser.",
  );
  expect(webWords(source(), "home", false, SITE)?.title).toMatch(/^Opens the password manager's web interface, inside knobas\./);
  expect(webWords(source(), "entry", true, { ...SITE, own: "" })).toBeNull();
  expect(webWords(source(), "entry", true, undefined)).toBeNull();
});
