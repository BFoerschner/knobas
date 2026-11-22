/**
 * The saved web sign-in (ADR-0050, decisions §13 (b), ux29): Settings' *Saved sign-in* row and its dialog, and the
 * site bar's *Fill sign-in* and what it says after a fill.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { SiteAsked, SiteSetting, SiteStatus } from "../../ipc/site";
import type { SiteActionPorts } from "./actions";
import { createSiteStore, type SiteStore } from "./site.svelte";
import TfSiteBar from "./TfSiteBar.svelte";
import TfSiteSection, { type SiteSectionPorts } from "./TfSiteSection.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | undefined;
let store: SiteStore;
let calls: string[];

const SETTING: SiteSetting = {
  source: "wiki",
  open_inside: true,
  base: "https://wiki.example",
  own: "https://wiki.example",
  allowed: [],
  suggested: [],
  kept_since: null,
  offered: true,
  persists: true,
  sign_in_user: null,
  own_sign_in: null,
};

function status(over: Partial<SiteStatus> = {}): SiteStatus {
  return {
    source: "wiki",
    phase: "live",
    url: "https://wiki.example/login.action",
    title: "Log in",
    own: true,
    loading: false,
    can_back: false,
    can_forward: false,
    shown: true,
    keyboard: "site",
    asks_origin: null,
    failure: null,
    esc_again_ms: 500,
    fill: false,
    ...over,
  };
}

const done: SiteAsked = { done: true, asks: null };
const actions: SiteActionPorts = {
  close: () => Promise.resolve(done),
  act: () => Promise.resolve(done),
  open: () => Promise.resolve(done),
  fill: (source) => {
    calls.push(`fill ${source}`);
    return Promise.resolve();
  },
};

const ports: SiteSectionPorts = {
  setOpenInside: () => Promise.resolve(SETTING),
  allow: () => Promise.resolve(SETTING),
  disallow: () => Promise.resolve(SETTING),
  forget: () => Promise.resolve(done),
  saveSignIn: (id, username, password) => {
    calls.push(`save ${id} ${username} ${password}`);
    return Promise.resolve({ ...SETTING, sign_in_user: username });
  },
  removeSignIn: (id) => {
    calls.push(`remove ${id}`);
    return Promise.resolve({ ...SETTING, sign_in_user: null, own_sign_in: store.settings.get(id)?.own_sign_in ?? null });
  },
  ownSignIn: (id, on) => {
    calls.push(`own ${id} ${on}`);
    return Promise.resolve({ ...SETTING, own_sign_in: { user: "mara", on } });
  },
};

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  store = createSiteStore();
  store.hold([SETTING]);
  calls = [];
});

afterEach(() => {
  if (view) unmount(view);
  view = undefined;
  target.remove();
  document.body.replaceChildren();
});

function bar(over: Partial<SiteStatus>) {
  view = mount(TfSiteBar, { target, props: { source: "wiki", name: "Wiki", status: status(over), setting: SETTING, back: null, store, actions } });
  flushSync();
}

const button = (label: string) => [...document.querySelectorAll("button")].find((el) => el.textContent?.trim() === label);

test("the bar draws Fill sign-in on the source's own page with a saved sign-in, and a press fills", () => {
  bar({ fill: true });
  const fill = button("Fill sign-in");
  expect(fill).toBeDefined();
  fill!.click();
  expect(calls).toEqual(["fill wiki"]);
});

test("no saved sign-in, or a fill under way, draws no live button", () => {
  bar({ fill: false });
  expect(button("Fill sign-in")).toBeUndefined();
  unmount(view!);
  bar({ fill: false, sign_in: "filling" });
  expect(button("Filling…")?.disabled).toBe(true);
});

test("a sign-in the site did not take is said where the address was", () => {
  bar({ fill: true, sign_in: "not_taken" });
  expect(target.querySelector(".tf-site-says")?.textContent).toBe("Wiki did not take the saved sign-in.");
  expect(target.querySelector(".tf-site-at")).toBeNull();
});

test("a code is the person's to type", () => {
  bar({ sign_in: "second_factor" });
  expect(target.querySelector(".tf-site-says")?.textContent).toBe("Wiki asks for a code. That one is yours to type.");
});

test("on a provider's page the bar says knobas does not fill there, and offers no fill", () => {
  bar({ own: false, url: "https://idp.example/sso", sign_in: "provider" });
  expect(target.querySelector(".tf-chip.tf-warn")?.textContent).toBe("not Wiki's own address, so its sign-in is not filled");
  expect(button("Fill sign-in")).toBeUndefined();
  expect(target.querySelector(".tf-site-host")?.textContent).toBe("idp.example");
});

function section() {
  view = mount(TfSiteSection, { target, props: { source: "wiki", name: "Wiki", ports, store } });
  flushSync();
}

function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

test("Settings saves a sign-in from the dialog, then names it and never shows the password", async () => {
  section();
  const row = [...target.querySelectorAll(".tf-stg-row")].find((el) => el.textContent?.includes("Saved sign-in"));
  expect(row?.textContent).toContain("None");
  button("Save one…")!.click();
  flushSync();
  const user = document.querySelector<HTMLInputElement>("#tf-site-signin-user")!;
  const pass = document.querySelector<HTMLInputElement>("#tf-site-signin-pass")!;
  expect(pass.type).toBe("password");
  const save = button("Save sign-in")!;
  expect(save.disabled).toBe(true);
  type(user, " knobas ");
  type(pass, "s3cr3t");
  flushSync();
  expect(save.disabled).toBe(false);
  document.querySelector<HTMLFormElement>("#tf-site-signin")!.requestSubmit();
  await tick();
  await tick();
  flushSync();
  expect(calls).toEqual(["save wiki knobas s3cr3t"]);
  expect(document.querySelector("#tf-site-signin-pass")).toBeNull();
  expect(target.textContent).toContain("Saved for knobas, never shown");
  expect(document.body.innerHTML).not.toContain("s3cr3t");
});

test("Change opens the dialog with the name and an empty password; Remove removes", async () => {
  store.holdOne({ ...SETTING, sign_in_user: "knobas" });
  section();
  button("Change…")!.click();
  flushSync();
  expect(document.querySelector<HTMLInputElement>("#tf-site-signin-user")!.value).toBe("knobas");
  expect(document.querySelector<HTMLInputElement>("#tf-site-signin-pass")!.value).toBe("");
  button("Cancel")!.click();
  flushSync();
  button("Remove")!.click();
  await tick();
  await tick();
  flushSync();
  expect(calls).toEqual(["remove wiki"]);
  expect(button("Save one…")).toBeDefined();
});

test("a site that signs in with its source's credential says so, and turns off and on", async () => {
  store.holdOne({ ...SETTING, own_sign_in: { user: "mara", on: true } });
  section();
  const row = () => [...target.querySelectorAll(".tf-stg-row")].find((el) => el.textContent?.includes("Saved sign-in"))!;
  expect(row().textContent).toContain("Same as Wiki's credential, mara");
  expect(button("Save one…")).toBeUndefined();
  button("Turn off")!.click();
  await tick();
  await tick();
  flushSync();
  expect(calls).toEqual(["own wiki false"]);
  expect(row().textContent).toContain("Off. You sign in yourself.");
  button("Turn on")!.click();
  await tick();
  await tick();
  flushSync();
  expect(calls).toEqual(["own wiki false", "own wiki true"]);
  expect(row().textContent).toContain("Same as Wiki's credential, mara");
});

test("Use another… saves a sign-in that wins, and Remove goes back to the source's credential", async () => {
  store.holdOne({ ...SETTING, own_sign_in: { user: "mara", on: true } });
  section();
  button("Use another…")!.click();
  flushSync();
  expect(document.body.textContent).toContain("Save another sign-in for Wiki");
  expect(document.querySelector<HTMLInputElement>("#tf-site-signin-user")!.value).toBe("");
  type(document.querySelector<HTMLInputElement>("#tf-site-signin-user")!, "admin");
  type(document.querySelector<HTMLInputElement>("#tf-site-signin-pass")!, "pw");
  flushSync();
  document.querySelector<HTMLFormElement>("#tf-site-signin")!.requestSubmit();
  await tick();
  await tick();
  flushSync();
  store.holdOne({ ...store.settings.get("wiki")!, own_sign_in: { user: "mara", on: true } });
  flushSync();
  expect(target.textContent).toContain("Saved for admin, never shown");
  button("Remove")!.click();
  await tick();
  await tick();
  flushSync();
  expect(target.textContent).toContain("Same as Wiki's credential, mara");
});
