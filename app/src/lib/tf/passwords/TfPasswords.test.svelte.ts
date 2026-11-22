import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AttachmentAnswer, Held, PwEntry, PwRow, ReadAnswer, SecretRef, WriteAnswer } from "../../ipc/passwords";
import TfPasswords from "./TfPasswords.svelte";
import { forgetHeld } from "./held.svelte";
import { forgetComment, type PasswordPorts } from "./ports";
import { resetClip } from "./clip.svelte";

const offers = { one_time_codes: true, expiry: true, usage_comments: true, audit_log: true, history: null, attachments: false, access_requests: false, marks_protected_fields: false, writes: [] };

function row(id: string, folder: string, title: string, flags = 0): PwRow {
  return [id, 0, folder, title, `${title.split(" ")[0]}_user`, null, "2026-08-30T10:22:00Z", flags, null, []];
}

function held(down: string | null = null): Held {
  return {
    sources: [{ id: "pw", name: "Team passwords", account: "mara.lindqvist", by_hand: true, synced_at: "2026-11-22T10:22:00Z", down_at: down, never_synced: false, offers, skipped_root: "pw:root", adapter_name: "Pleasant Password Server" }],
    folders: [
      { id: "pw:pay", source: 0, parent: null, name: "Payments", comment_read: true, can_add: true },
      { id: "pw:agents", source: 0, parent: "pw:pay", name: "Agents", comment_read: false, can_add: true },
      { id: "pw:plat", source: 0, parent: null, name: "Platform", comment_read: false, can_add: true },
    ],
    entries: [
      row("pw:payout", "pw:pay", "payout db", 4 | 1),
      ...Array.from({ length: 8 }, (_, i) => row(`pw:a${i}`, "pw:agents", `agent ${i}`)),
      row("pw:graf", "pw:plat", "grafana"),
    ],
    recent: [],
  };
}

function record(id: string, over: Partial<PwEntry> = {}): PwEntry {
  return {
    id, source: "pw", title: id === "pw:payout" ? "payout db" : id, username: "payout_user", address: "https://pay.example", folder: id === "pw:payout" ? "pw:pay" : "pw:plat",
    notes: "Rotated quarterly.", fields: [{ name: "Port", value: "5432" }, { name: "Recovery PIN", value: null }],
    otp: id === "pw:payout" ? { format: "keepass", digits: 6, period: 30, algorithm: "SHA1" } : null,
    expires: null, changed: "2026-08-30T10:22:00Z", changed_by: "Priya Nair",
    comment: { read: id === "pw:payout", modify: false }, approval: { state: "none" }, attachments: { state: "none" }, can_read: true, can_modify: false,
    ...over,
  };
}

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;

function ports(over: Partial<PasswordPorts> = {}, view: Held = held()): PasswordPorts & { calls: { copy: [string, SecretRef, string | null][] } } {
  const calls = { copy: [] as [string, SecretRef, string | null][] };
  return {
    calls,
    held: vi.fn(async () => view),
    entry: vi.fn(async (id: string) => record(id)),
    show: vi.fn(async (): Promise<ReadAnswer> => ({ kind: "shown", value: "Tq8#mV", read_at: "2026-11-22T10:23:07Z", until: new Date(Date.now() + 30_000).toISOString() })),
    copy: vi.fn(async (id: string, which: SecretRef, comment: string | null): Promise<ReadAnswer> => {
      calls.copy.push([id, which, comment]);
      return { kind: "copied", read_at: "2026-11-22T10:23:07Z", until: new Date(Date.now() + 30_000).toISOString() };
    }),
    hide: vi.fn(async () => {}),
    code: vi.fn(async () => ({ kind: "code" as const, code: "482913", digits: 6, period: 30, left_ms: 12_000 })),
    copyCode: vi.fn(async () => ({ kind: "code" as const, code: "482913", digits: 6, period: 30, left_ms: 12_000 })),
    copyPlain: vi.fn(async () => {}),
    openExternal: vi.fn(async () => {}),
    write: vi.fn(async (): Promise<WriteAnswer> => ({ kind: "written", entity_id: "pw:graf" })),
    copySecret: vi.fn(async () => {}),
    attachment: vi.fn(async (): Promise<AttachmentAnswer> => ({ kind: "read", name: "wg-hub.conf", size: 21, data: btoa("[Interface]\nPort = 1"), text: "[Interface]\nPort = 1" })),
    saveAttachment: vi.fn(async () => "/home/mara/Downloads/wg-hub.conf"),
    ...over,
  };
}

async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));
    flushSync();
  }
}

async function open(p: PasswordPorts, rest: string[] = [], timedTicket: string | null = null): Promise<void> {
  app = mount(TfPasswords, { target, props: { ready: true, rest, ports: p, timedTicket } });
  await settle();
}

const text = (sel: string) => [...target.querySelectorAll(sel)].map((el) => el.textContent?.replace(/\s+/g, " ").trim() ?? "");
/** The button `label` in the entry's row named `field`. */
const inRow = (field: string, label: string) =>
  [...target.querySelectorAll<HTMLElement>(".tf-pw-entry .tf-pw-f")]
    .find((row) => row.querySelector(".tf-pw-fl")?.textContent?.trim() === field)!
    .querySelector<HTMLButtonElement>(`button[class*="tf-btn"]:is(${["Show", "Hide"].includes(label) ? ":first-of-type" : ":nth-of-type(2), :only-of-type"})`)!;
const button = (label: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim().startsWith(label))!;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  forgetHeld();
  forgetComment();
  resetClip();
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

test("the tree lists the folders under the skipped root, with their totals and the comment mark", async () => {
  await open(ports());
  expect(text(".tf-pw-node .tf-pw-nm")).toEqual(["All entries", "Payments", "Platform"]);
  expect(text(".tf-pw-node .tf-pw-n")).toEqual(["10", "9", "1"]);
  expect(target.querySelectorAll(".tf-pw-node .tf-pw-mark")).toHaveLength(1);
  expect(target.querySelector("h1")?.textContent).toBe("Passwords");
  expect(target.querySelector(".tf-sub")?.textContent).toContain("10 entries in 3 folders. Synced by hand at");
});

test("a folder's own entries come first, a long folder below shows five and Show all, and the first entry opens", async () => {
  await open(ports(), ["in", "pw:pay"]);
  expect(text(".tf-pw-lhead h3")).toEqual(["Directly in Payments", "Agents"]);
  // Agents holds eight: five drawn, then *Show all*.
  expect(target.querySelectorAll(".tf-pw-row")).toHaveLength(6);
  expect(button("Show all 8 in Agents")).toBeTruthy();
  expect(target.querySelector(".tf-pw-entry h2")?.textContent).toBe("payout db");
  // The code is made here: asked once when the entry opens.
  expect(text(".tf-pw-code")).toEqual(["482913"]);
  expect(target.querySelector(".tf-pw-otp .tf-pw-fh")?.textContent).toContain("Made on this machine. Works offline.");
  // A field the listing did not send is a secret row; one it sent is plain.
  expect(text(".tf-pw-entry .tf-pw-fl")).toEqual(["Username", "Password", "One-time code", "Address", "Notes", "Port", "Recovery PIN"]);
});

test("the filter matches every folder and marks the letters; Esc clears it", async () => {
  await open(ports());
  const filter = target.querySelector<HTMLInputElement>(".tf-pw-filter input")!;
  filter.value = "agent 3";
  filter.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(text(".tf-pw-lhead h3")).toEqual(["1 entry in every folder"]);
  expect(text(".tf-pw-row .tf-pw-t")).toEqual(["agent 3"]);
  expect(target.querySelectorAll(".tf-pw-row mark").length).toBeGreaterThan(0);
  filter.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await settle();
  expect(filter.value).toBe("");
});

test("copying a password in a comment-required folder asks first, pre-filled with the timed ticket, and Enter sends it", async () => {
  const p = ports();
  await open(p, ["entry", "pw:payout"], "PAY-231");
  expect(target.querySelector(".tf-pw-f .tf-pw-fh")?.textContent).toContain("Asks for a usage comment first: Payments requires one.");
  inRow("Password", "Copy").click();
  await settle();
  expect(p.copy).not.toHaveBeenCalled();
  const input = target.querySelector<HTMLInputElement>(".tf-pw-ask input")!;
  expect(input.value).toBe("PAY-231");
  expect(target.querySelector(".tf-pw-ask p")?.textContent).toContain("Payments requires one for every read. The password manager keeps it in its log, under mara.lindqvist.");
  input.form!.requestSubmit();
  await settle();
  expect(p.calls.copy).toEqual([["pw:payout", { kind: "password" }, "PAY-231"]]);
  // The next prompt starts with the comment last sent.
  expect(target.querySelector(".tf-pw-f .tf-pw-fh")?.textContent).toContain("On the clipboard. It clears in");
  expect(target.querySelector(".tf-pw-f .tf-pw-fh")?.textContent).toContain("recorded it under mara.lindqvist, with your comment.");
});

test("showing a password draws it with its comb and covers it again on Hide", async () => {
  const p = ports();
  await open(p, ["entry", "pw:graf"]);
  button("Show").click();
  await settle();
  expect(target.querySelector(".tf-pw-clear")?.textContent).toBe("Tq8#mV");
  expect(target.querySelector(".tf-pw-comb")).toBeTruthy();
  expect(target.querySelector(".tf-pw-f .tf-pw-fh")?.textContent).toContain("Hides itself in");
  button("Hide").click();
  await settle();
  expect(target.querySelector(".tf-pw-clear")).toBeNull();
  expect(p.hide).toHaveBeenCalledWith("pw:graf", { kind: "password" });
});

test("a source that does not answer keeps its entries; reads are turned off and codes still come", async () => {
  const p = ports({}, held("2026-11-22T10:40:00Z"));
  await open(p, ["entry", "pw:payout"]);
  expect(target.querySelector(".tf-sub")?.textContent).toContain("did not answer at");
  expect(target.querySelector(".tf-stale")?.textContent).toContain("did not answer at");
  expect(inRow("Password", "Show").disabled).toBe(true);
  expect(inRow("Password", "Copy").disabled).toBe(true);
  expect(inRow("Password", "Show").title).toBe("Team passwords does not answer");
  expect(text(".tf-pw-code")).toEqual(["482913"]);
  // With one source there is no top row to dot: the header says it.
});

test("the keys: c copies the password, u the username, o the code; r does nothing", async () => {
  const p = ports();
  await open(p, ["entry", "pw:graf"]);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "c", bubbles: true }));
  await settle();
  expect(p.copy).toHaveBeenCalledTimes(1);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "u", bubbles: true }));
  await settle();
  expect(p.copyPlain).toHaveBeenCalledWith("payout_user");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "r", bubbles: true }));
  await settle();
  expect(p.held).toHaveBeenCalledTimes(1);
});

test("no password source says how to add one; one never synced offers its sync", async () => {
  await open(ports({}, { sources: [], folders: [], entries: [], recent: [] }));
  expect(target.querySelector("h1")?.textContent).toBe("No password source yet");
  unmount(app!);
  app = null;
  forgetHeld();
  const unsynced = held();
  unsynced.sources[0] = { ...unsynced.sources[0]!, never_synced: true, synced_at: null };
  await open(ports({}, unsynced));
  expect(target.querySelector("h1")?.textContent).toBe("Team passwords is not synced yet");
  expect(target.textContent).toContain("It syncs only when you press Sync now. A sync lists its entries and their folders. It reads no passwords.");
});

/** A source that performs the three writes, and an entry the account may change. */
function writable(down: string | null = null): Held {
  const view = held(down);
  view.sources[0] = { ...view.sources[0]!, offers: { ...offers, writes: ["create_entry", "edit_entry", "set_password"] } };
  return view;
}

test("Change password… sends the new value once, with a comment where the entry asks one, and opens the entry again", async () => {
  const p = ports({ entry: vi.fn(async (id: string) => record(id, { can_modify: true, comment: { read: true, modify: true } })) }, writable());
  await open(p, ["entry", "pw:graf"], "PLAT-7");
  button("Change password…").click();
  await settle();
  const dialog = target.ownerDocument.querySelector<HTMLFormElement>(".tf-pw-form")!;
  const value = dialog.querySelector<HTMLInputElement>("#tf-pw-new")!;
  // It starts with a generated password.
  expect(value.value.length).toBeGreaterThanOrEqual(16);
  expect(dialog.querySelector<HTMLInputElement>("#tf-pw-comment")!.value).toBe("PLAT-7");
  expect(target.ownerDocument.body.textContent).toContain("It does not change it at pay.example");
  value.value = "n3w-Secret";
  value.dispatchEvent(new Event("input", { bubbles: true }));
  dialog.requestSubmit();
  await settle();
  expect(p.write).toHaveBeenCalledTimes(1);
  expect(p.write).toHaveBeenCalledWith({ write: "set_password", entity: "pw:graf", password: "n3w-Secret", comment: "PLAT-7", listed_in: "pw:plat" });
  expect(target.ownerDocument.querySelector(".tf-pw-form")).toBeNull();
});

test("Edit sends only what changed, and no password", async () => {
  const p = ports({ entry: vi.fn(async (id: string) => record(id, { can_modify: true })) }, writable());
  await open(p, ["entry", "pw:graf"]);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", bubbles: true }));
  await settle();
  const dialog = target.ownerDocument.querySelector<HTMLFormElement>(".tf-pw-form")!;
  expect(dialog.querySelector("#tf-pw-new")).toBeNull();
  const user = dialog.querySelector<HTMLInputElement>("#tf-pw-user")!;
  user.value = "grafana_admin";
  user.dispatchEvent(new Event("input", { bubbles: true }));
  dialog.requestSubmit();
  await settle();
  expect(p.write).toHaveBeenCalledWith({ write: "edit_entry", entity: "pw:graf", comment: null, listed_in: "pw:plat", username: "grafana_admin" });
});

test("a write the source does not answer keeps the dialog and what was typed, and says nothing was saved", async () => {
  const p = ports({ write: vi.fn(async (): Promise<WriteAnswer> => ({ kind: "unreachable", message: "unreachable" })) }, writable());
  await open(p, ["in", "pw:plat"]);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true }));
  await settle();
  const dialog = target.ownerDocument.querySelector<HTMLFormElement>(".tf-pw-form")!;
  expect(dialog.querySelector<HTMLSelectElement>("#tf-pw-folder")!.value).toBe("pw:plat");
  const title = dialog.querySelector<HTMLInputElement>("#tf-pw-title")!;
  title.value = "loki";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  dialog.requestSubmit();
  await settle();
  expect(p.write).toHaveBeenCalledTimes(1);
  expect(target.ownerDocument.querySelector(".tf-pw-form #tf-pw-title")).not.toBeNull();
  expect(target.ownerDocument.querySelector<HTMLInputElement>("#tf-pw-title")!.value).toBe("loki");
  expect(target.ownerDocument.body.textContent).toContain("Team passwords does not answer, so nothing was saved.");
});

test("no write is offered where the source performs none, the account may not change the entry, or the source is down", async () => {
  await open(ports({ entry: vi.fn(async (id: string) => record(id, { can_modify: true })) }), ["entry", "pw:graf"]);
  expect(button("Change password…")).toBeUndefined();
  expect(button("New entry")).toBeUndefined();
  unmount(app!);
  app = null;
  forgetHeld();
  await open(ports({}, writable()), ["entry", "pw:graf"]);
  expect(button("Change password…")).toBeUndefined();
  expect(button("New entry")).toBeTruthy();
  unmount(app!);
  app = null;
  forgetHeld();
  await open(ports({ entry: vi.fn(async (id: string) => record(id, { can_modify: true })) }, writable("2026-11-22T10:40:00Z")), ["entry", "pw:graf"]);
  expect(button("Change password…").disabled).toBe(true);
  expect(button("New entry")).toBeUndefined();
});

test("an entry's attachments are listed by name and size; Open reads one live into the viewer, Save puts it in Downloads", async () => {
  const listed = { state: "listed" as const, files: [{ id: "a1", name: "wg-hub.conf", size: 612 }] };
  const p = ports({ entry: vi.fn(async (id: string) => record(id, { attachments: listed })) }, (() => {
    const v = held();
    v.sources[0] = { ...v.sources[0]!, offers: { ...offers, attachments: true } };
    return v;
  })());
  await open(p, ["entry", "pw:graf"]);
  const row = target.querySelector(".tf-pw-file")!;
  expect([...row.querySelectorAll(".tf-tkkind, .tf-t, .tf-rsub")].map((el) => el.textContent)).toEqual(["CONF", "wg-hub.conf", "612 B"]);
  // Nothing is read until Open is pressed.
  expect(p.attachment).not.toHaveBeenCalled();
  [...row.querySelectorAll("button")].find((b) => b.textContent === "Open")!.click();
  await settle();
  expect(p.attachment).toHaveBeenCalledWith("pw:graf", "a1");
  expect(document.body.querySelector(".tf-viewer")?.textContent).toContain("[Interface]");
  [...row.querySelectorAll("button")].find((b) => b.textContent === "Save")!.click();
  await settle();
  expect(p.saveAttachment).toHaveBeenCalledWith("pw:graf", "a1");
});

test("a source that offers no attachments draws no row, whatever the record says", async () => {
  const listed = { state: "listed" as const, files: [{ id: "a1", name: "wg-hub.conf", size: 612 }] };
  await open(ports({ entry: vi.fn(async (id: string) => record(id, { attachments: listed })) }), ["entry", "pw:graf"]);
  expect(target.querySelector(".tf-pw-file")).toBeNull();
});

test("a read the server refuses for this account says the server's own sentence", async () => {
  const reason = 'Access denied. "mara.lindqvist" does not have permission to view the password for "grafana".';
  const p = ports({ show: vi.fn(async (): Promise<ReadAnswer> => ({ kind: "refused", reason })) });
  await open(p, ["entry", "pw:graf"]);
  button("Show").click();
  await settle();
  expect(target.querySelector(".tf-pw-f .tf-pw-fh")?.textContent).toContain(reason);
});

test("New entry is drawn only where the account may add to a folder", async () => {
  const none = writable();
  none.folders = none.folders.map((f) => ({ ...f, can_add: false }));
  await open(ports({}, none), ["in", "pw:plat"]);
  expect(button("New entry")).toBeUndefined();
});

test("a read the source does not answer reads the page again, so every read turns off with the reason", async () => {
  const p = ports({ copy: vi.fn(async (): Promise<ReadAnswer> => ({ kind: "unreachable", message: "unreachable" })) });
  await open(p, ["entry", "pw:graf"]);
  expect(p.held).toHaveBeenCalledTimes(1);
  inRow("Password", "Copy").click();
  await settle();
  expect(target.querySelector(".tf-pw-f .tf-pw-fh")?.textContent).toContain("did not answer, so nothing was read");
  expect(p.held).toHaveBeenCalledTimes(2);
});
