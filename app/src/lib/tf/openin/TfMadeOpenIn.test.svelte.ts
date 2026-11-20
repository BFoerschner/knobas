/** *Open in…* on a generated folder (ADR-0046): its rows, `Enter` on the default, a `{url}` command disabled, *Copy path*. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CheckoutSettings } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { createApps, type Apps } from "./apps.svelte";
import TfMadeOpenIn from "./TfMadeOpenIn.svelte";

const DIR = "/Users/mara/code/knobas-rendered/0123/order-flow.site.0123abcd";
const settings = (): CheckoutSettings => ({
  clones_root: "/Users/mara/code",
  git: { status: "found", path: "/usr/bin/git" },
  default_application: "vscode",
  applications: [
    { id: "finder", name: "Finder", command: "open {path}", file_command: null },
    { id: "web", name: "Web page", command: "open {url}", file_command: null },
    { id: "vscode", name: "VS Code", command: "code {path}", file_command: "code --goto {file}:{line}" },
  ],
});

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
let apps: Apps;
let ran: [string | null, string, string | null, number | null][];
let copied: string[];
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  toasts.items = [];
  ran = [];
  copied = [];
  apps = createApps({
    checkoutSettings: () => Promise.resolve(settings()),
    setCheckoutApplications: () => Promise.resolve(settings()),
    openCheckout: () => Promise.reject(new Error("never a checkout")),
  });
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function draw(props: Record<string, unknown>) {
  app = mount(TfMadeOpenIn, {
    target,
    props: {
      token: "t1",
      dir: DIR,
      apps,
      run: (application: string | null, token: string, file: string | null, line: number | null) => {
        ran.push([application, token, file, line]);
        return Promise.resolve(["code"]);
      },
      copy: (text: string) => {
        copied.push(text);
        return Promise.resolve();
      },
      ...props,
    },
  });
  flushSync();
}

const face = () => target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")!;
const items = () => [...document.querySelectorAll<HTMLButtonElement>(".tf-appmenu [role='menuitem']")];

test("the menu lists each application with its command filled on the generated file, the default first", async () => {
  draw({ file: "docs/index.md", line: 4 });
  await settle();
  face().click();
  await settle();
  expect(items().map((b) => b.querySelector("b")?.textContent)).toEqual(["VS Code (default)", "Finder", "Web page", "Copy path"]);
  expect(items()[0]!.textContent).toContain(`code --goto ${DIR}/docs/index.md:4`);
  expect(items()[1]!.textContent).toContain(`open ${DIR}`);
  const web = items()[2]!;
  expect(web.disabled).toBe(true);
  expect(web.title).toBe("Web page's command needs a page on the source, and a generated folder has none");
  // The default is focused: Enter on it runs it.
  items()[0]!.click();
  await settle();
  expect(ran).toEqual([["vscode", "t1", "docs/index.md", 4]]);
});

test("Copy path copies the file's absolute path, or the folder's on the landing, and says so", async () => {
  draw({ file: null });
  await settle();
  face().click();
  await settle();
  items().find((b) => b.textContent?.startsWith("Copy path"))!.click();
  await settle();
  expect(copied).toEqual([DIR]);
  expect(toasts.items.at(-1)?.text).toBe(`Copied ${DIR}`);
});

test("before a folder is kept the button is there, disabled, holding its room", () => {
  draw({ token: null, dir: null });
  expect(face().disabled).toBe(true);
  expect(face().textContent).toContain("Open in…");
});
