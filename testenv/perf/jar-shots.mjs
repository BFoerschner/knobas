// Screenshots of the JAR view (ux19) for its design pass, over the built app under ?fake-ipc&fake-jar: a library JAR's
// head and tree, a class opened, it decompiled, the fat JAR's dependency list with a library open, in light and dark.
//
//   node testenv/perf/jar-shots.mjs --attach <port> --app <url> --out <dir> [--size WxH@s]
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { attach, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const size = sizeFrom(args);
const out = arg("--out", ".");
const c = await attach(Number(arg("--attach", "9222")));
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service/files/";
const click = (text, sel = ".tf-jar-rbtn, button") =>
  c.js(`(() => { const b = [...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.textContent.trim().startsWith(${JSON.stringify(text)})); if (!b) throw new Error("no ${text}"); b.click(); })()`);
const shot = async (name) => {
  await sleep(500);
  const { data } = await c.send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(out, `${name}-${size.name}.png`), Buffer.from(data, "base64"));
  console.log(`${name}-${size.name}.png`);
};
for (const scheme of ["light", "dark"]) {
  await c.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
  await c.send("Page.navigate", { url: `${arg("--app", "http://127.0.0.1:5340")}/?fake-ipc&fake-jar#/inbox` });
  await sleep(2500);
  await c.js(`location.hash = ${JSON.stringify(REPO + "build/libs/ledger-core-2.3.1.jar")}`);
  await sleep(1500);
  await shot(`${scheme}-plain`);
  await click("com/tidewater/ledger/");
  await click("Ledger.class");
  await sleep(300);
  await c.js(`document.querySelector('.tf-jar-detail')?.scrollIntoView({ block: 'center' })`);
  await shot(`${scheme}-class`);
  await click("Source", ".tf-seg button");
  await sleep(600);
  await shot(`${scheme}-source`);
  await c.js(`location.hash = ${JSON.stringify(REPO + "build/libs/payout-service-0.0.1-SNAPSHOT.jar")}`);
  await sleep(1500);
  await shot(`${scheme}-fat`);
  await click("Dependencies", ".tf-seg button");
  await click("spring-core");
  await sleep(300);
  await shot(`${scheme}-deps`);
}
process.exit(0);
