// The schema explorer's stills (`docs/decisions/2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-
// is-made-from-it.md`): states of the built app under ?fake-ipc&fake-xml&fake-xsd, the page only, in a headless Chrome.
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/xsd-shots.mjs --port <port> --out <dir> [--size WxH@s] [--theme light|dark] [--only a,b]
import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { attach, launch, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const port = Number(arg("--port", "5340"));
const out = arg("--out", ".");
const theme = arg("--theme", "dark");
const only = arg("--only", null)?.split(",") ?? null;
const size = sizeFrom(args);
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";
const file = (path, at = null) => (at === null ? `${REPO}/files/${path}` : `${REPO}/at/${encodeURIComponent(at)}/${path}`);

const { port: debug, stop } = await launch({ size });
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
await c.send("Page.navigate", { url: `http://127.0.0.1:${port}/?fake-ipc&fake-xml&fake-xsd#/inbox` });
// The app is up once its tabs are drawn.
for (let i = 0; i < 60; i++) {
  if (await c.js(`!!document.querySelector('.tf-tab, nav a[href^="#/"]')`)) break;
  await sleep(500);
}
await sleep(1500);

const STATES = [
  { name: "01-overview-opentrans", hash: file("schemas/opentrans/opentrans_2_1.xsd") },
  { name: "02-element-order", hash: file("schemas/opentrans/opentrans_2_1.xsd", "el/ORDER") },
  { name: "03-rows-open-order-header", hash: file("schemas/opentrans/opentrans_2_1.xsd", "el/ORDER"), then: `() => { const r = [...document.querySelectorAll('.tf-oas-fr.tf-oas-can')].find((x) => x.querySelector('.tf-oas-fname').textContent.trim() === 'ORDER_HEADER'); r.click(); }` },
  { name: "05-followed-into-bmecat", hash: file("schemas/opentrans/opentrans_2_1.xsd", "el/bmecat:PRODUCT_DETAILS") },
  { name: "06-type-currencies", hash: file("schemas/opentrans/opentrans_2_1.xsd", "type/bmecat:dtCURRENCIES") },
  { name: "06b-type-pattern", hash: file("schemas/opentrans/opentrans_2_1.xsd", "type/dtBOOLEAN") },
  { name: "07-filter", hash: file("schemas/opentrans/opentrans_2_1.xsd"), then: `() => { const b = document.querySelector('.tf-oas-railhead input'); b.value = 'price amount'; b.dispatchEvent(new Event('input', { bubbles: true })); }` },
  { name: "08-not-found-here", hash: file("schemas/mixed/opentrans_2_1.xsd") },
  { name: "09-ubl-invoice", hash: file("ubl/xsd/maindoc/UBL-Invoice-2.1.xsd", "el/Invoice") },
  { name: "09b-ubl-overview", hash: file("ubl/xsd/maindoc/UBL-Invoice-2.1.xsd") },
  { name: "10-cii-overview", hash: file("cii/data/standard/CrossIndustryInvoice_100pD16B.xsd") },
  { name: "11-types-mode", hash: file("schemas/opentrans/opentrans_2_1.xsd"), then: `() => [...document.querySelectorAll('.tf-oas-modes button')].find((b) => b.textContent === 'Types').click()` },
].filter((s) => only === null || only.includes(s.name));

for (const s of STATES) {
  await c.js(`(() => { location.hash = ${JSON.stringify(file("README.md"))}; })()`);
  await sleep(600);
  await c.js(`(() => { location.hash = ${JSON.stringify(s.hash)}; })()`);
  await sleep(2500);
  if (s.then) {
    await c.js(`(${s.then})()`);
    await sleep(1200);
  }
  const shot = await c.send("Page.captureScreenshot", { format: "png" });
  const path = join(out, `${size.name}-${theme}-${s.name}.png`);
  writeFileSync(path, Buffer.from(shot.data, "base64"));
  console.log(path);
}
stop();
