// A look at the run dialog's places in the built app under ?fake-ipc&fake-oneoff: prints what stands where, for a probe
// that cannot find its selector. `with-built.sh <built> <port> node testenv/perf/oneoff-look.mjs --port <port> <hash>`
import { attach, launch, sleep, viewport, DEFAULT_SIZE } from "./cdp.mjs";
const args = process.argv.slice(2);
const port = Number(args[args.indexOf("--port") + 1]);
const hash = args[args.length - 1];
const { port: debug, stop } = await launch({ size: DEFAULT_SIZE });
const c = await attach(debug);
await viewport(c, DEFAULT_SIZE);
await c.send("Page.enable");
await c.send("Runtime.enable");
const said = [];
c.on?.("Runtime.exceptionThrown", (e) => said.push(e.exceptionDetails?.exception?.description?.split("\n")[0] ?? "exception"));
c.on?.("Runtime.consoleAPICalled", (e) => { if (e.type === "error") said.push(e.args.map((a) => a.value ?? a.description).join(" ").slice(0, 300)); });
await c.send("Page.navigate", { url: `http://127.0.0.1:${port}/?fake-ipc&fake-oneoff#/inbox` });
await sleep(2500);
await c.js(`location.hash = ${JSON.stringify(hash)}`);
await sleep(5000);
console.log(await c.js(`JSON.stringify({ heads: [...document.querySelectorAll("header")].map((h) => h.className + " :: " + h.textContent.replace(/\\s+/g, " ").slice(0, 160)), runin: [...document.querySelectorAll(".tf-oo-runin")].map((e) => { const p = []; for (let n = e.parentElement; n && p.length < 5; n = n.parentElement) p.push(n.tagName + "." + [...n.classList].join(".")); return p.join(" < "); }), trails: [...document.querySelectorAll(".tf-trail")].map((t) => t.className + " :: " + t.textContent.trim().slice(0, 80)), files: [...document.querySelectorAll("[class*=fileview], .tf-file")].map((e) => e.className).slice(0, 5), url: location.hash, body: document.body.innerText.slice(0, 300) })`));
console.log(said.slice(0, 10).join("\n"));
const { data } = await c.send("Page.captureScreenshot", { format: "png" });
(await import("node:fs")).writeFileSync(process.env.LOOK_SHOT ?? "/dev/null", Buffer.from(data, "base64"));
c.close();
stop();
