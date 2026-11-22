/**
 * The saved web sign-in's fill (ADR-0050, decisions §13 (b), ux29): the very script `crates/knobas-site/src/fill-sign-in.js`
 * that knobas runs in a site, against each built-in kind's recorded sign-in page (`crates/knobas-site/tests/sign-in-pages/`, recorded from the
 * live testenv products) and the tricky ones: a change-password form, a form in a frame, a hidden password field, a
 * one-time code, and a page with no form.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { afterEach, expect, test } from "vitest";

const SCRIPT = readFileSync(join(process.cwd(), "..", "crates", "knobas-site", "src", "fill-sign-in.js"), "utf8").trim();
const PAGES = join(process.cwd(), "..", "crates", "knobas-site", "tests", "sign-in-pages");

/** The script as knobas calls it: the function applied to its three JSON string literals. */
function run(username: string, password: string, mode: "fill" | "look"): unknown {
  return (0, eval)(`(${SCRIPT})(${JSON.stringify(username)},${JSON.stringify(password)},${JSON.stringify(mode)})`);
}

let submits: HTMLFormElement[] = [];
const onSubmit = (event: Event) => {
  event.preventDefault();
  submits.push(event.target as HTMLFormElement);
};

function page(html: string) {
  document.documentElement.innerHTML = html;
  submits = [];
  document.addEventListener("submit", onSubmit, true);
}

afterEach(() => {
  document.removeEventListener("submit", onSubmit, true);
  document.documentElement.innerHTML = "<head></head><body></body>";
});

function body(file: string): string {
  const html = readFileSync(join(PAGES, file), "utf8");
  return html.replace(/^[\s\S]*?<html[^>]*>/i, "").replace(/<\/html>[\s\S]*$/i, "");
}

const PASSWORD = `pa"ss\\word'<x>`;

/** Each built-in kind's page: the fields the fill types into and the "remember me" box it ticks. */
const KINDS: { file: string; user: string; password: string; remember: string }[] = [
  { file: "gitea.html", user: "#user_name", password: "#password", remember: "input[name=remember]" },
  { file: "forgejo.html", user: "#user_name", password: "#password", remember: "input[name=remember]" },
  { file: "jira.html", user: "#username-field", password: "#password-field", remember: "input[name=rememberMe]" },
  { file: "confluence.html", user: "#username-field", password: "#password-field", remember: "input[name=rememberMe]" },
  { file: "teamcity.html", user: "#username", password: "#password", remember: "input[name=remember]" },
  { file: "kuma.html", user: "input[autocomplete=username]", password: "input[type=password]", remember: "#remember" },
  // An external adapter's site (knobas-adapter-xwiki, `knobas:adapter@1.6.0`).
  { file: "xwiki.html", user: "#j_username", password: "#j_password", remember: "input[name=j_rememberme]" },
];

for (const kind of KINDS) {
  test(`${kind.file}: fills both fields, ticks "remember me" and submits once`, () => {
    page(body(kind.file));
    const typed: string[] = [];
    document.addEventListener("input", (event) => typed.push((event.target as HTMLInputElement).value), true);
    expect(run("knobas", PASSWORD, "look")).toBe("form");
    expect((document.querySelector(kind.password) as HTMLInputElement).value).toBe("");
    expect(submits).toHaveLength(0);

    expect(run("knobas", PASSWORD, "fill")).toBe("filled");
    expect((document.querySelector(kind.user) as HTMLInputElement).value).toBe("knobas");
    expect((document.querySelector(kind.password) as HTMLInputElement).value).toBe(PASSWORD);
    expect((document.querySelector(kind.remember) as HTMLInputElement).checked).toBe(true);
    // A framework listening for `input` (Vue's v-model, React's onChange) saw both values.
    expect(typed).toEqual(expect.arrayContaining(["knobas", PASSWORD]));
    expect(submits).toHaveLength(1);
  });
}

test("a change-password form is left alone", () => {
  page(`<form><input name="user" value="me"><input type="password" name="old"><input type="password" name="new"><button>Change</button></form>`);
  expect(run("knobas", PASSWORD, "fill")).toBe("ambiguous");
  for (const field of document.querySelectorAll("input[type=password]")) expect((field as HTMLInputElement).value).toBe("");
  expect(submits).toHaveLength(0);
});

test("a form inside a frame is not the top document's", () => {
  page(`<p>Sign in below</p><iframe srcdoc="<form><input name=u><input type=password name=p><button>Go</button></form>"></iframe>`);
  expect(run("knobas", PASSWORD, "fill")).toBe("no form");
  expect(submits).toHaveLength(0);
});

test("a hidden password field alone is no sign-in form", () => {
  page(`<form><input name="u"><input type="password" name="p" style="display:none"><button>Go</button></form>`);
  expect(run("knobas", PASSWORD, "fill")).toBe("no form");
  page(`<form><input name="u"><div hidden><input type="password" name="p"></div><button>Go</button></form>`);
  expect(run("knobas", PASSWORD, "fill")).toBe("no form");
});

test("a hidden password trap beside the real field is not counted, and gets nothing", () => {
  page(
    `<form><input name="u"><input type="password" name="trap" style="visibility:hidden"><input type="password" name="p"><button>Go</button></form>`,
  );
  expect(run("knobas", PASSWORD, "fill")).toBe("filled");
  expect((document.querySelector("[name=p]") as HTMLInputElement).value).toBe(PASSWORD);
  expect((document.querySelector("[name=trap]") as HTMLInputElement).value).toBe("");
});

test("a page with no form, or no name field before the password, fills nothing", () => {
  page(`<main><h1>Dashboard</h1><input type="search" placeholder="Search"></main>`);
  expect(run("knobas", PASSWORD, "fill")).toBe("no form");
  page(`<form><input type="password" name="p"><input name="after"><button>Go</button></form>`);
  expect(run("knobas", PASSWORD, "fill")).toBe("no form");
  expect((document.querySelector("[name=p]") as HTMLInputElement).value).toBe("");
});

test("a one-time code is the person's to type", () => {
  page(`<form><input name="otp" autocomplete="one-time-code"><button>Verify</button></form>`);
  expect(run("knobas", PASSWORD, "fill")).toBe("second factor");
  // Uptime Kuma asks for its code in the same form as the password.
  page(`<form><input autocomplete="username"><input type="password"><input id="otp" placeholder="123456" autocomplete="one-time-code"><button type="submit">Log in</button></form>`);
  expect(run("knobas", PASSWORD, "fill")).toBe("second factor");
  expect((document.querySelector("input[type=password]") as HTMLInputElement).value).toBe("");
  expect(submits).toHaveLength(0);
});

test("a ticked box stays ticked and an unrelated box is left alone", () => {
  page(
    `<form><input name="u"><input type="password" name="p"><label><input type="checkbox" name="terms"> I agree</label><label><input type="checkbox" name="keep" checked> Keep me signed in</label><button>Go</button></form>`,
  );
  expect(run("knobas", PASSWORD, "fill")).toBe("filled");
  expect((document.querySelector("[name=terms]") as HTMLInputElement).checked).toBe(false);
  expect((document.querySelector("[name=keep]") as HTMLInputElement).checked).toBe(true);
});

test("a form with no submit button is submitted through its own submit event", () => {
  page(`<form><input name="u"><input type="password" name="p"></form>`);
  expect(run("knobas", PASSWORD, "fill")).toBe("filled");
  expect(submits).toHaveLength(1);
});
