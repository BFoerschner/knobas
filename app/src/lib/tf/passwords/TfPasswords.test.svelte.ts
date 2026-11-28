import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

// The filter ranks through the app's ranking (#2535): here a letters-in-order stand-in.
vi.mock("../../ipc/search", async (actual) => ({ ...(await actual<object>()), ...(await import("../filter/test-fuzzy")).fuzzyIpc() }));

import type { AttachmentAnswer, Held, PwEntry, PwRow, ReadAnswer, SecretRef, WriteAnswer } from "../../ipc/passwords";
import TfPasswords from "./TfPasswords.svelte";
import { forgetHeld } from "./held.svelte";
import { forgetComment, type PasswordPorts } from "./ports";
import { resetClip } from "./clip.svelte";
import { siteStore } from "../site/site.svelte";
import { onMac } from "../../shell/platform";
import { reset as resetToasts, toasts } from "../../shell/toasts.svelte";
import type { SiteSetting } from "../../ipc/site";

/** The password source's site, as Settings holds it (ux30): it opens inside and signs in with the source's credential. */
const SITE: SiteSetting = {
  source: "pw", open_inside: true, base: "https://pw.example:10001", own: "https://pw.example:10001", allowed: [], suggested: [],
  kept_since: null, offered: true, persists: true, sign_in_user: null, own_sign_in: { user: "mara.lindqvist", on: true }, confined: true,
};

const offers = { one_time_codes: true, expiry: true, usage_comments: true, audit_log: true, history: null, attachments: false, access_requests: false, marks_protected_fields: false, writes: [] };

function row(id: string, folder: string, title: string, flags = 0): PwRow {
  return [id, 0, folder, title, `${title.split(" ")[0]}_user`, null, "2026-08-30T10:22:00Z", flags, null, []];
}

function held(down: string | null = null): Held {
  return {
    sources: [{ id: "pw", name: "Team passwords", account: "mara.lindqvist", by_hand: true, synced_at: "2026-11-22T10:22:00Z", down_at: down, never_synced: false, offers, skipped_root: "pw:root", adapter_name: "Pleasant Password Server", web: { entry: true, folder: true } }],
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
    write: vi.fn(async (): Promise<WriteAnswer> => ({ kind: "written", entity_id: "pw:graf", path_unread: false })),
    copySecret: vi.fn(async () => {}),
    attachment: vi.fn(async (): Promise<AttachmentAnswer> => ({ kind: "read", name: "wg-hub.conf", size: 21, data: btoa("[Interface]\nPort = 1"), text: "[Interface]\nPort = 1" })),
    saveAttachment: vi.fn(async () => "/home/mara/Downloads/wg-hub.conf"),
    webLink: vi.fn(async (_source: string, item: string | null) => ({ url: `https://pw.example:10001/web?item=${item ?? ""}`, at: item === null ? ("home" as const) : ("entry" as const) })),
    openWeb: vi.fn(() => {}),
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
  siteStore.hold([SITE]);
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
  siteStore.hold([]);
});

test("the tree lists the folders under the skipped root, with their totals and the comment mark", async () => {
  await open(ports());
  expect(text(".tf-pw-node .tf-pw-nm")).toEqual(["All entries", "Payments", "Platform"]);
  expect(text(".tf-pw-node .tf-pw-n")).toEqual(["10", "9", "1"]);
  expect(target.querySelectorAll(".tf-pw-node .tf-pw-mark")).toHaveLength(1);
  expect(target.querySelector("h1")?.textContent).toBe("Secrets");
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

test("the keys: c copies the password, u the username, o the code; r does nothing; / does not swallow section search", async () => {
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
  // `/` belongs to the shell's section search (#2515), not the local filter.
  const slash = new KeyboardEvent("keydown", { key: "/", bubbles: true, cancelable: true });
  window.dispatchEvent(slash);
  await settle();
  expect(slash.defaultPrevented).toBe(false);
  expect(document.activeElement).not.toBe(target.querySelector(".tf-pw-filter input"));
  expect(target.querySelector(".tf-pw-filter kbd")?.textContent).toMatch(/^(⌘F|Ctrl\+F)$/);
});

test("no secrets source says how to add one; one never synced offers its sync", async () => {
  await open(ports({}, { sources: [], folders: [], entries: [], recent: [] }));
  expect(target.querySelector("h1")?.textContent).toBe("No secrets source yet");
  expect(target.querySelector(".tf-empty a")?.textContent).toBe("Add a secrets source");
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

test("an entry moved to another folder says its path shows after the source's next sync, and one kept in place says nothing more (#2596)", async () => {
  const moved = vi.fn(async (): Promise<WriteAnswer> => ({ kind: "written", entity_id: "pw:graf", path_unread: true }));
  const p = ports({ entry: vi.fn(async (id: string) => record(id, { can_modify: true })), write: moved }, writable());
  await open(p, ["entry", "pw:graf"]);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", bubbles: true }));
  await settle();
  target.ownerDocument.querySelector<HTMLFormElement>(".tf-pw-form")!.requestSubmit();
  await settle();
  // Team passwords syncs by hand, so what brings the path is Sync now.
  expect(toasts.items.map((t) => t.text)).toContain("Saved the entry. Its path shows after Sync now on Team passwords.");
  resetToasts();
  moved.mockResolvedValueOnce({ kind: "written", entity_id: "pw:graf", path_unread: false });
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", bubbles: true }));
  await settle();
  target.ownerDocument.querySelector<HTMLFormElement>(".tf-pw-form")!.requestSubmit();
  await settle();
  expect(toasts.items.map((t) => t.text)).toEqual(["Saved the entry."]);
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

test("the open entry offers its web interface, inside and signed in, and a press opens the address the Rust side builds", async () => {
  const p = ports();
  await open(p, ["entry", "pw:graf"]);
  const web = [...target.querySelectorAll<HTMLButtonElement>(".tf-pw-entry .tf-acts button")].find((b) => b.textContent?.startsWith("Open web interface"))!;
  expect(web.textContent).toBe("Open web interfaceW");
  expect(web.title).toBe(
    "Opens this entry in the password manager's web interface, inside knobas and signed in. A new sign-in is recorded there under mara.lindqvist. Shift W opens it in your browser instead.",
  );
  web.click();
  await settle();
  expect(p.webLink).toHaveBeenCalledWith("pw", "pw:graf");
  expect(p.openWeb).toHaveBeenCalledWith("https://pw.example:10001/web?item=pw:graf", "pw", false, "pw:graf");
});

test("W opens the open entry's web interface, Shift W in the browser", async () => {
  const p = ports();
  await open(p, ["entry", "pw:graf"]);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "w", bubbles: true }));
  await settle();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "W", shiftKey: true, bubbles: true }));
  await settle();
  expect((p.openWeb as ReturnType<typeof vi.fn>).mock.calls.map((c) => [c[0], c[2]])).toEqual([
    ["https://pw.example:10001/web?item=pw:graf", false],
    ["https://pw.example:10001/web?item=pw:graf", true],
  ]);
});

test("the folder on show offers its own web page in the header; the source's top its front page", async () => {
  const p = ports();
  await open(p, ["in", "pw:plat"]);
  const head = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-pw-head .tf-acts button")].find((b) => b.textContent?.startsWith("Open web interface"));
  expect(head()!.title).toMatch(/^Opens this folder in the password manager's web interface, inside knobas and signed in\./);
  head()!.click();
  await settle();
  expect(p.webLink).toHaveBeenLastCalledWith("pw", "pw:plat");
  unmount(app!);
  app = null;
  await open(p, []);
  expect(head()!.title).toMatch(/^Opens the password manager's web interface,/);
  head()!.click();
  await settle();
  expect(p.webLink).toHaveBeenLastCalledWith("pw", null);
});

test("a source whose base URL is no web address, or with its site off, draws no button or says the browser", async () => {
  siteStore.hold([{ ...SITE, own: "" }]);
  await open(ports(), ["entry", "pw:graf"]);
  expect(button("Open web interface")).toBeUndefined();
  unmount(app!);
  app = null;
  siteStore.hold([{ ...SITE, open_inside: false }]);
  await open(ports(), ["entry", "pw:graf"]);
  const web = [...target.querySelectorAll<HTMLButtonElement>(".tf-pw-entry .tf-acts button")].find((b) => b.textContent?.startsWith("Open web interface"))!;
  expect(web.title).toBe("Opens this entry in the password manager's web interface, in your browser.");
});

test("the first open sorts the folders whose rows it draws, not every folder; ↓ walks on into the folders below", async () => {
  // 400 folders of six: the top lists a panel for each. At 20,000 entries sorting them all before the first frame was
  // most of the first open (the ux30 end gate, 2026-11-23); the window draws a few dozen lines.
  const many: Held = {
    ...held(),
    folders: Array.from({ length: 400 }, (_, f) => ({ id: `pw:f${f}`, source: 0, parent: null, name: `folder ${String(f).padStart(3, "0")}`, comment_read: false, can_add: true })),
    entries: Array.from({ length: 2400 }, (_, i) => row(`pw:e${i}`, `pw:f${i % 400}`, `entry ${String((i * 7919) % 2400).padStart(4, "0")}`)),
  };
  const sorted: number[] = [];
  const sort = Array.prototype.sort;
  const spy = vi.spyOn(Array.prototype, "sort").mockImplementation(function (this: unknown[], compare) {
    if (Array.isArray(this[0]) || (this[0] as { title?: unknown } | undefined)?.title !== undefined) sorted.push(this.length);
    return sort.call(this, compare);
  });
  // No frame runs until the count is read: two frames after the draw, the overscan grows and the filter's index sorts
  // every entry in runs, which a loaded machine let in before the read and counted against the first open (#2542).
  vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame"] });
  try {
    await open(ports({}, many));
    const drawn = target.querySelectorAll(".tf-pw-lhead").length;
    expect(drawn).toBeGreaterThan(2);
    expect(sorted.length).toBeLessThanOrEqual(drawn + 1);
    expect(sorted.length).toBeLessThan(50);
    vi.advanceTimersByTime(100);
    vi.useRealTimers();
    await settle();
    // The first entry opens; ↓ goes through the first folder's rows and on into the next.
    const first = target.querySelector(".tf-pw-row[aria-current=true] .tf-pw-t")?.textContent;
    expect(first).toBe(text(".tf-pw-row .tf-pw-t")[0]);
    for (let i = 0; i < 6; i++) window.dispatchEvent(new KeyboardEvent("keydown", { key: "j" }));
    await settle();
    expect(target.querySelector(".tf-pw-row[aria-current=true] .tf-pw-t")?.textContent).toBe(text(".tf-pw-row .tf-pw-t")[6]);
  } finally {
    vi.useRealTimers();
    spy.mockRestore();
  }
});

test("a key that acts on the entry, pressed while the entry just picked is read, waits and acts on that one", async () => {
  let answer: (() => void) | null = null;
  const p = ports({
    entry: vi.fn((id: string) => (id === "pw:a1" ? new Promise<PwEntry>((done) => (answer = () => done(record(id)))) : Promise.resolve(record(id)))),
  });
  await open(p, ["in", "pw:pay"]);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "j" }));
  await settle();
  expect(target.querySelector(".tf-pw-entry h2")?.textContent).toBe("pw:a0");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "j" }));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }));
  await settle();
  // pw:a1 is not read yet: the pane still draws pw:a0, and nothing of pw:a0 is shown.
  expect(target.querySelector(".tf-pw-entry h2")?.textContent).toBe("pw:a0");
  expect(p.show).not.toHaveBeenCalled();
  answer!();
  await settle();
  expect(target.querySelector(".tf-pw-entry h2")?.textContent).toBe("pw:a1");
  expect(vi.mocked(p.show).mock.calls.map((call) => call[0])).toEqual(["pw:a1"]);
});

test("the one-time code's type is made once the page is idle, in a hidden sample that takes no room", async () => {
  await open(ports(), ["in", "pw:pay"]);
  const frame = () => new Promise((done) => requestAnimationFrame(() => done(true)));
  await frame();
  await frame();
  await new Promise((done) => setTimeout(done, 1100));
  flushSync();
  const sample = target.querySelector(".tf-pw-entrycol > .tf-pw-warm");
  expect(sample?.getAttribute("aria-hidden")).toBe("true");
  expect(sample?.querySelector(".tf-pw-code")?.textContent).toBe("000000");
});

test("leaving the page after the filter's index has had its first frame drops the second one too", async () => {
  // The index starts two frames after the page draws. Its teardown cancelled only the first frame's handle, so a page
  // left after that frame had run kept the second one asking: under jsdom an animation frame is a 16 ms interval, which
  // light CI's residue pass caught (`residue.test.svelte.ts`, TfPasswords between answers, run 45151236493).
  const frames = new Map<number, FrameRequestCallback>();
  let handle = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.set(++handle, callback);
    return handle;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => void frames.delete(id));
  try {
    await open(ports());
    const first = [...frames.values()];
    frames.clear();
    for (const callback of first) callback(performance.now());
    flushSync();
    expect(frames.size).toBeGreaterThan(0);

    unmount(app!);
    app = null;
    expect([...frames.keys()]).toEqual([]);
  } finally {
    vi.unstubAllGlobals();
  }
});

/** Mounted with props that change after the mount, as the router changes `rest`. */
async function openLive(p: PasswordPorts, rest: string[]): Promise<{ rest: string[] }> {
  const props = $state({ ready: true, rest, ports: p, timedTicket: null as string | null });
  app = mount(TfPasswords, { target, props });
  await settle();
  return props;
}

const filterBox = () => target.querySelector<HTMLInputElement>(".tf-pw-filter input")!;
async function type(value: string): Promise<void> {
  filterBox().value = value;
  filterBox().dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
}
const node = (name: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-pw-node")].find((b) => b.querySelector(".tf-pw-nm")?.textContent === name)!;

test("in a folder the filter reads only that folder and the folders under it, and says so (#2531)", async () => {
  await open(ports(), ["in", "pw:pay"]);
  expect(filterBox().placeholder).toBe("Filter Payments");
  // "a" is in grafana (Platform) too, which is not under Payments.
  await type("a");
  expect(text(".tf-pw-lhead h3")).toEqual(["9 entries in Payments"]);
  expect(text(".tf-pw-row .tf-pw-t")).not.toContain("grafana");
  await type("grafana");
  expect(text(".tf-pw-blank p")[0]).toContain('Nothing in Payments matches "grafana".');
});

test("the app ranks every entry, held to the folder's scope, marks the letters it matched, and a key keeps what was drawn until its answer (#2535)", async () => {
  await open(ports());
  const before = text(".tf-pw-lhead h3");
  filterBox().value = "agnt3";
  filterBox().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  // The answer is out: the folders' panels stand as drawn, no list of the old filter in between.
  expect(text(".tf-pw-lhead h3")).toEqual(before);
  await settle();
  // No substring holds "agnt3"; its letters stand in order in "agent 3".
  const rows = [...target.querySelectorAll(".tf-pw-row .tf-pw-t")];
  expect(rows[0]?.textContent).toBe("agent 3");
  expect([...rows[0]!.querySelectorAll("mark")].map((m) => m.textContent).join("").toLowerCase()).toBe("agnt3");
  await type("grfn");
  expect(text(".tf-pw-row .tf-pw-t")).toContain("grafana");
  unmount(app!);
  app = null;

  await open(ports(), ["in", "pw:pay"]);
  // "grfn" holds in grafana, which is in Platform, not under Payments.
  await type("grfn");
  expect(text(".tf-pw-blank p")[0]).toContain('Nothing in Payments matches "grfn".');
});

test("another folder, by a press, a link or the address, clears the filter (#2531)", async () => {
  const props = await openLive(ports(), ["in", "pw:pay"]);
  await type("agent");
  // The address moves without a press of the tree: a crumb, a panel's head, Back.
  props.rest = ["in", "pw:plat"];
  await settle();
  expect(filterBox().value).toBe("");
  expect(text(".tf-pw-row .tf-pw-t")).toEqual(["grafana"]);
  await type("graf");
  node("Payments").click();
  await settle();
  expect(filterBox().value).toBe("");
});

test("the folder on show is drawn selected, also while the filter has words (#2531)", async () => {
  await open(ports(), ["in", "pw:pay"]);
  expect(node("Payments").getAttribute("aria-current")).toBe("true");
  expect(node("Platform").getAttribute("aria-current")).toBeNull();
  await type("agent");
  expect(node("Payments").getAttribute("aria-current")).toBe("true");
});

test("a press on a folder's row selects it and never folds it; the chevron and ← → fold (#2531)", async () => {
  const props = await openLive(ports(), []);
  const pressed = () => node("Payments").getAttribute("aria-expanded");
  node("Payments").querySelector<HTMLElement>(".tf-pw-nm")!.click();
  props.rest = ["in", "pw:pay"];
  await settle();
  // Select-only (spec #2514): a press on a closed folder selects it and leaves it closed.
  expect(location.hash).toBe("#/secrets/in/pw%3Apay");
  expect(node("Payments").getAttribute("aria-current")).toBe("true");
  expect(pressed()).toBe("false");
  node("Payments").querySelector<HTMLElement>("[data-twist]")!.click();
  await settle();
  expect(pressed()).toBe("true");
  // On the folder on show, open: a second press does not close it.
  node("Payments").querySelector<HTMLElement>(".tf-pw-nm")!.click();
  await settle();
  expect(pressed()).toBe("true");
  node("Payments").click();
  await settle();
  expect(pressed()).toBe("true");
  node("Payments").querySelector<HTMLElement>("[data-twist]")!.click();
  await settle();
  expect(pressed()).toBe("false");
  node("Payments").focus();
  node("Payments").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  await settle();
  expect(pressed()).toBe("true");
});

test("Alt+click on a folder opens everything under it, and again closes everything under it (#2531)", async () => {
  const deep = held();
  deep.folders.push({ id: "pw:linux", source: 0, parent: "pw:agents", name: "Linux", comment_read: false, can_add: true });
  deep.entries.push(row("pw:lx", "pw:linux", "linux agent"));
  location.hash = "#/secrets";
  await open(ports({}, deep));
  const alt = (name: string) => node(name).dispatchEvent(new MouseEvent("click", { bubbles: true, altKey: true, button: 0 }));
  alt("Payments");
  await settle();
  expect(text(".tf-pw-node .tf-pw-nm")).toEqual(["All entries", "Payments", "Agents", "Linux", "Platform"]);
  // Alt+click selects nothing: the address stays.
  expect(location.hash).toBe("#/secrets");
  alt("Payments");
  await settle();
  expect(text(".tf-pw-node .tf-pw-nm")).toEqual(["All entries", "Payments", "Platform"]);
  // Opened alone again, Agents keeps the closed state Alt+click gave it.
  node("Payments").querySelector<HTMLElement>("[data-twist]")!.click();
  await settle();
  expect(text(".tf-pw-node .tf-pw-nm")).toEqual(["All entries", "Payments", "Agents", "Platform"]);
});

/** ⌘ on the Mac, Ctrl elsewhere, as the platform under test reads it. */
const mod = (key: string, over: KeyboardEventInit = {}) =>
  new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...(onMac() ? { metaKey: true } : { ctrlKey: true }), ...over });

test("in the filter, ⌘⇧C, U, O, S and A act on the open entry; ⌘C and ⌘A stay the box's (#2531)", async () => {
  const p = ports();
  await open(p, ["in", "pw:plat"]);
  await type("graf");
  filterBox().focus();
  const plainCopy = mod("c");
  filterBox().dispatchEvent(plainCopy);
  const selectAll = mod("a");
  filterBox().dispatchEvent(selectAll);
  await settle();
  expect(plainCopy.defaultPrevented).toBe(false);
  expect(selectAll.defaultPrevented).toBe(false);
  expect(p.copy).not.toHaveBeenCalled();
  expect(p.openExternal).not.toHaveBeenCalled();
  // Shift makes the key a capital: the chord is read either way.
  const copy = mod("C", { shiftKey: true });
  filterBox().dispatchEvent(copy);
  await settle();
  expect(copy.defaultPrevented).toBe(true);
  expect(p.calls.copy).toEqual([["pw:graf", { kind: "password" }, null]]);
  filterBox().dispatchEvent(mod("U", { shiftKey: true }));
  await settle();
  expect(p.copyPlain).toHaveBeenCalledWith("payout_user");
  filterBox().dispatchEvent(mod("S", { shiftKey: true }));
  await settle();
  expect(p.show).toHaveBeenCalledTimes(1);
  filterBox().dispatchEvent(mod("A", { shiftKey: true }));
  await settle();
  expect(p.openExternal).toHaveBeenCalledWith("https://pay.example");
  // The filter keeps focus and its words throughout.
  expect(document.activeElement).toBe(filterBox());
  expect(filterBox().value).toBe("graf");
});

test("⌘⇧O in the filter copies the open entry's one-time code (#2531)", async () => {
  const p = ports();
  await open(p, ["in", "pw:pay"]);
  await type("payout");
  filterBox().focus();
  filterBox().dispatchEvent(mod("O", { shiftKey: true }));
  await settle();
  expect(p.copyCode).toHaveBeenCalledTimes(1);
});

test("Enter in the filter takes focus to the open entry's row, where the bare keys answer (#2531)", async () => {
  const p = ports();
  await open(p, ["in", "pw:pay"]);
  await type("agent 3");
  filterBox().focus();
  const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  filterBox().dispatchEvent(enter);
  await settle();
  expect(enter.defaultPrevented).toBe(true);
  const focused = document.activeElement as HTMLElement;
  expect(focused.classList.contains("tf-pw-row")).toBe(true);
  expect(focused.getAttribute("aria-current")).toBe("true");
  expect(focused.querySelector(".tf-pw-t")?.textContent).toBe("agent 3");
  focused.dispatchEvent(new KeyboardEvent("keydown", { key: "c", bubbles: true, cancelable: true }));
  await settle();
  expect(p.calls.copy.map((c) => c[0])).toEqual(["pw:a3"]);
});

test("Enter in the filter focuses the first match's row when the entry opened by its address is filtered out (#2531)", async () => {
  // Opened from the launcher: the address names the entry, and the router answers a new address only later.
  const view = held();
  view.entries.push(row("pw:kib", "pw:plat", "kibana"));
  await open(ports({}, view), ["entry", "pw:graf"]);
  await type("kib");
  expect(text(".tf-pw-row .tf-pw-t")).toEqual(["kibana"]);
  filterBox().focus();
  filterBox().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  await settle();
  const focused = document.activeElement as HTMLElement;
  expect(focused.classList.contains("tf-pw-row")).toBe(true);
  expect(focused.querySelector(".tf-pw-t")?.textContent).toBe("kibana");
  expect(filterBox().value).toBe("kib");
});

test("Enter in the filter before the ranking answers opens the first match of that answer, not a row of the panels drawn before (#2535)", async () => {
  await open(ports(), ["in", "pw:plat"]);
  filterBox().value = "grfn";
  filterBox().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  // The answer is out: the folder's panels still stand, grafana not first among them.
  filterBox().focus();
  filterBox().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  await settle();
  const focused = document.activeElement as HTMLElement;
  expect(focused.classList.contains("tf-pw-row")).toBe(true);
  expect(focused.querySelector(".tf-pw-t")?.textContent).toBe("grafana");
  // Typed on before an answer: that Enter is dropped, and nothing more is opened for the new words.
  const opened = location.hash;
  filterBox().focus();
  filterBox().value = "grfnx";
  filterBox().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  filterBox().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  // Back to words grafana holds: a stale Enter would take focus to its row once they are answered.
  filterBox().value = "grfn";
  filterBox().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  await settle();
  expect(location.hash).toBe(opened);
  expect(document.activeElement).toBe(filterBox());
});

test("the filter's chords read Ctrl as ⌘ and a layout without Latin letters by the key's place (#2531)", async () => {
  const p = ports();
  await open(p, ["in", "pw:plat"]);
  await type("graf");
  filterBox().focus();
  filterBox().dispatchEvent(new KeyboardEvent("keydown", { key: "C", ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true }));
  await settle();
  expect(p.calls.copy).toEqual([["pw:graf", { kind: "password" }, null]]);
  // Cyrillic С on the C key.
  filterBox().dispatchEvent(new KeyboardEvent("keydown", { key: "С", code: "KeyC", metaKey: true, shiftKey: true, bubbles: true, cancelable: true }));
  await settle();
  expect(p.calls.copy).toHaveLength(2);
  // A Latin letter on another place is read as typed: ⌘⇧Q is no chord where Q sits on the A key.
  const q = new KeyboardEvent("keydown", { key: "Q", code: "KeyA", metaKey: true, shiftKey: true, bubbles: true, cancelable: true });
  filterBox().dispatchEvent(q);
  await settle();
  expect(q.defaultPrevented).toBe(false);
  expect(p.openExternal).not.toHaveBeenCalled();
});

test("a long attachment name is one line that gives up its middle, its end and extension kept, the whole name its title (#2531)", async () => {
  const name = "payout-db client certificate bundle for the payments team 2026-11.p12";
  const listed = { state: "listed" as const, files: [{ id: "a1", name, size: 612 }] };
  const view = held();
  view.sources[0] = { ...view.sources[0]!, offers: { ...offers, attachments: true } };
  await open(ports({ entry: vi.fn(async (id: string) => record(id, { attachments: listed })) }, view), ["entry", "pw:graf"]);
  const box = target.querySelector<HTMLElement>(".tf-pw-file .tf-pw-filename > :first-child")!;
  expect(box.title).toBe(name);
  expect(box.classList.contains("tf-path")).toBe(true);
  expect(box.querySelector(".tf-path-head")?.textContent).toBe("payout-db client certificate bundle for the payments ");
  expect(box.querySelector(".tf-path-tail")?.textContent).toBe("team 2026-11.p12");
  // The size stays under the name, out of the cut.
  expect(box.textContent).toBe(name);
});
