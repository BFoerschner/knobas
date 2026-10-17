// One screenshot after steps, driven over the Chrome DevTools protocol (used by
// scripts/shot.sh when a route carries steps; issue #989).
// Usage: node scripts/shot-steps.mjs <port> <debug port> <profile> <size w,h> <theme> <file> <spec>
//   spec: `#/job/PAY-231 > #/builds + click:Some button`
//     ` > ` separates routes opened in turn inside one page (in-memory state
//     survives, so a job opened first scopes the next route);
//     ` + click:<name>` after a route clicks the first button, link, tab or menu
//     item whose accessible name or text is <name>; `click:testid=<id>` takes a
//     data-testid. Waits up to 5 s; a miss exits 1 naming the step.
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

const [port, debug, profile, size, theme, file, spec, extra = ""] = process.argv.slice(2);
const [width, height] = size.split(",").map(Number);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const flags = ["--headless=new", "--disable-gpu", "--hide-scrollbars", `--user-data-dir=${profile}`, `--remote-debugging-port=${debug}`, `--window-size=${size}`];
if (theme === "dark") flags.push("--blink-settings=preferredColorScheme=0");
if (theme === "light") flags.push("--blink-settings=preferredColorScheme=1");
const chrome = spawn("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", [...flags, "about:blank"], { stdio: "ignore" });
const done = (code, msg) => {
  if (msg) console.error(msg);
  chrome.kill();
  process.exit(code);
};
try {
  let page = null;
  for (let i = 0; i < 60 && !page; i++) {
    try {
      page = (await (await fetch(`http://localhost:${debug}/json`)).json()).find((t) => t.type === "page");
    } catch {}
    if (!page) await sleep(250);
  }
  if (!page) done(1, "shot: Chrome never offered a page");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let seq = 0;
  const waiting = new Map();
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && waiting.has(m.id)) (waiting.get(m.id)(m), waiting.delete(m.id));
  };
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      const id = ++seq;
      waiting.set(id, resolve);
      ws.send(JSON.stringify({ id, method, params }));
    });
  const evaluate = async (expression) => {
    const a = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    return a.result?.result?.value;
  };
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  const base = `http://localhost:${port}/?fake-ipc${extra}`;
  const clickJs = (name) => {
    const byId = name.startsWith("testid=");
    const want = JSON.stringify(byId ? name.slice(7) : name);
    return `(() => {
      const want = ${want};
      const nodes = ${byId}
        ? [...document.querySelectorAll("[data-testid]")].filter((x) => x.getAttribute("data-testid") === want)
        : [...document.querySelectorAll("button, a, summary, [role=button], [role=menuitem], [role=tab]")].filter(
            (x) => (x.getAttribute("aria-label") ?? x.textContent).trim() === want);
      const el = nodes.find((x) => !x.disabled && x.getAttribute("aria-disabled") !== "true");
      if (!el) return false;
      el.click();
      return true;
    })()`;
  };
  let first = true;
  for (const segment of spec.split(" > ")) {
    const [route, ...steps] = segment.split(" + ").map((s) => s.trim());
    const path = route.replace(/^#?\/?/, "");
    if (first) {
      await send("Page.navigate", { url: `${base}#/${path}` });
      await sleep(4000);
      first = false;
    } else {
      await evaluate(`location.hash = ${JSON.stringify("#/" + path)}`);
      await sleep(1500);
    }
    for (const step of steps) {
      if (!step.startsWith("click:")) done(1, `shot: unknown step '${step}' (known: click:<name>, click:testid=<id>)`);
      const name = step.slice(6);
      let hit = false;
      for (let i = 0; i < 20 && !hit; i++) {
        hit = await evaluate(clickJs(name));
        if (!hit) await sleep(250);
      }
      if (!hit) done(1, `shot: step '${step}' found no enabled element on #/${path}`);
      await sleep(500);
    }
  }
  await sleep(600);
  const { result } = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(file, Buffer.from(result.data, "base64"));
  done(0);
} catch (e) {
  done(1, `shot: ${e.message}`);
}
