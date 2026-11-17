// Screenshots of the OpenAPI view for a look (ux18): the small spec's head, an operation opened, a ref followed, the
// band stuck after a scroll, and the 5,000-operation spec, in the theme asked.
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/openapi-shots.mjs --port <port> --out <dir> [--size WxH@s] [--theme dark]
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
const theme = arg("--theme", "light");
const size = sizeFrom(args);
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";

const { port: debug, stop } = await launch({ size });
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
await c.send("Page.navigate", { url: `http://127.0.0.1:${port}/?fake-ipc&fake-openapi#/inbox` });
await sleep(2500);

const shot = async (name) => {
  const { data } = await c.send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(out, `${theme}-${name}.png`), Buffer.from(data, "base64"));
};
const js = (code) => c.js(code);

await js(`location.hash = ${JSON.stringify(`${REPO}/files/api/openapi.yaml`)}`);
await sleep(1500);
await shot("1-head");
await js(`[...document.querySelectorAll('.tf-oas-opbtn')].find((b) => b.textContent.includes('/pets/{petId}')).click()`);
await sleep(600);
await js(`document.querySelector('.tf-oas-detail').scrollIntoView({ block: 'start' })`);
await sleep(400);
await shot("2-operation");
await js(`document.querySelector('.tf-oas-detail .tf-oas-top .tf-oas-ref').click()`);
await sleep(600);
await shot("3-followed");
await js(`location.hash = ${JSON.stringify(`${REPO}/files/api/split.yaml`)}`);
await sleep(1500);
await js(`[...document.querySelectorAll('.tf-oas-opbtn')].find((b) => b.textContent.includes('/orders')).click()`);
await sleep(600);
await shot("4-other-file");
await js(`location.hash = ${JSON.stringify(`${REPO}/files/api/big.yaml`)}`);
await sleep(3000);
await js(`(() => { let b = document.querySelector('.tf-oas-list').parentElement; while (b && !/(auto|scroll)/.test(getComputedStyle(b).overflowY)) b = b.parentElement; (b ?? document.scrollingElement).scrollTop += 4000; })()`);
await sleep(600);
await shot("5-big-scrolled");
stop();
