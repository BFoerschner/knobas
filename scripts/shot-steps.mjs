// One screenshot after steps, driven over the Chrome DevTools protocol (used by
// scripts/shot.sh when a route carries steps; issue #989).
// Usage: node scripts/shot-steps.mjs <port> <debug port> <profile> <size w,h> <theme> <file> <spec>
//   spec: `#/job/PAY-231 > #/builds + click:Some button`
//     ` > ` separates routes opened in turn inside one page (in-memory state
//     survives, so a job opened first scopes the next route);
//     ` + click:<name>` after a route clicks the first button, link, tab or menu
//     item whose accessible name or text is <name> (failing that, whose first child
//     element's text is, so an item with a dim description matches); `click:testid=<id>` takes a
//     data-testid. Waits up to SHOT_STEP_WAIT_S; a miss exits 1 naming the step.
//   The other steps (#1396), for what a click cannot reach:
//     `focus:<name>` focuses the first enabled input, textarea, select or contenteditable
//       whose aria-label, placeholder or label text is <name>; `focus:testid=<id>`. Waits as click does.
//     `type:<text>` inserts <text> into the focused element (Input.insertText, so a
//       binding sees an input event). The text cannot hold ` + ` or ` > `.
//     `key:<combo>` presses one key: `key:Meta+k`, `key:Escape`, `key:Enter`, `key:j`,
//       `key:Shift+Tab`. Modifiers Meta, Control, Alt, Shift; a bare character also types.
//     `select:<name>=<option>` picks the option whose text (or value) is <option> in the first
//       enabled select whose aria-label or label text is <name>; `select:testid=<id>=<option>`.
//       Headless Chrome on macOS will not change a select from a key press.
//     `hover:<name>` moves the pointer to the centre of the element click would take (testid=
//       too), and leaves it there, so a control revealed on hover is in the capture. Only an
//       element with a box counts; failing a control, the innermost element whose whole text
//       is <name> (a row's key cell). Waits as click does; a miss exits 1 naming the step.
//     `dblclick:<name>` double-clicks there with the pointer (a row or card that opens on it).
//     `back:<name>` / `forward:<name>` press mouse button 3 / 4 there (#693's navigation).
//       These four go through Input.dispatchMouseEvent, so the page sees real pointer events;
//       click: stays a DOM click.
//     `wait:<ms>` pauses, for a debounce or an animation.
//     `scroll:<px>` scrolls the page's largest scrolling element by <px> (negative goes up);
//       `scroll:testid=<id>` scrolls that element into view.
//   SHOT_FULL=1 grows the viewport to the tallest scrolling element's content before the
//   capture (at most 8000 px), so a long page is one image.
//   The waits (#1957) poll for the state they need and run out only on a guard sized
//   past a loaded machine's, each named in its failure: SHOT_START_WAIT_S (default 120,
//   whole seconds) for Chrome's DevTools endpoint to offer a page, the first route to
//   finish loading and any one DevTools command to be answered; SHOT_STEP_WAIT_S
//   (default 60) for the element a step names. A probe that hangs is cut off at its
//   guard too. A Chrome that exits before it offers a page fails at once. SHOT_CHROME
//   replaces the Chrome binary (scripts/shot-test.sh starts a late, a dead and a mute one);
//   unset, scripts/chrome-path.mjs finds it (the macOS app, else Chrome or Chromium on PATH).
//   Every uncaught exception and console.error in the page is printed as `shot: uncaught …`
//   or `shot: console.error …`. An uncaught exception fails the shot (exit 1) after its image
//   is written, so a screen that throws never passes as a picture; SHOT_ALLOW_ERRORS=1 lets
//   it through.
import { spawn } from "node:child_process";
import { chromePath as findChrome } from "./chrome-path.mjs";
import { writeFileSync } from "node:fs";

const [port, debug, profile, size, theme, file, spec, extra = ""] = process.argv.slice(2);
const [width, height] = size.split(",").map(Number);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// A guard in whole seconds from the environment, or its default; anything else is
// refused, as shot.sh refuses it.
const guard = (name, fallback) => {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return fallback;
  if (!/^[1-9][0-9]*$/.test(raw)) {
    console.error(`shot: ${name} must be a whole number of seconds, got '${raw}'`);
    process.exit(2);
  }
  return Number(raw);
};
const START_WAIT_S = guard("SHOT_START_WAIT_S", 120);
const STEP_WAIT_S = guard("SHOT_STEP_WAIT_S", 60);
// `promise`'s value, or `fallback` once `ms` pass with it unsettled.
const bounded = (ms, promise, fallback = null) => {
  let timer;
  const out = new Promise((r) => (timer = setTimeout(() => r(fallback), ms)));
  return Promise.race([promise, out]).finally(() => clearTimeout(timer));
};
// Polls `probe` every 250 ms until it returns something truthy, or the guard runs
// out (null); a probe still unanswered at the guard is given up on (at least 1 s
// for the last). `stop` is asked between polls for a reason to give up at once.
const poll = async (seconds, probe, stop = () => null) => {
  const deadline = Date.now() + seconds * 1000;
  for (;;) {
    const got = await bounded(Math.max(1000, deadline - Date.now()), probe());
    if (got) return got;
    if (stop() || Date.now() >= deadline) return null;
    await sleep(250);
  }
};
// The keys a `key:` step can name beyond one character: [key, code, keyCode, text].
const NAMED_KEYS = {
  Enter: ["Enter", "Enter", 13, "\r"],
  Escape: ["Escape", "Escape", 27],
  Tab: ["Tab", "Tab", 9],
  Backspace: ["Backspace", "Backspace", 8],
  Delete: ["Delete", "Delete", 46],
  Space: [" ", "Space", 32, " "],
  ArrowUp: ["ArrowUp", "ArrowUp", 38],
  ArrowDown: ["ArrowDown", "ArrowDown", 40],
  ArrowLeft: ["ArrowLeft", "ArrowLeft", 37],
  ArrowRight: ["ArrowRight", "ArrowRight", 39],
  Home: ["Home", "Home", 36],
  End: ["End", "End", 35],
  PageUp: ["PageUp", "PageUp", 33],
  PageDown: ["PageDown", "PageDown", 34],
};
// One printable character's physical key, for those that are not a letter or a digit.
const PUNCTUATION = {
  "/": ["Slash", 191], "?": ["Slash", 191], "[": ["BracketLeft", 219], "]": ["BracketRight", 221],
  ",": ["Comma", 188], ".": ["Period", 190], ";": ["Semicolon", 186], "-": ["Minus", 189],
  "=": ["Equal", 187], ">": ["Period", 190], "#": ["Digit3", 51], "@": ["Digit2", 50],
};
const MODIFIERS = { Alt: 1, Control: 2, Meta: 4, Shift: 8 };
// The steps that move the pointer, over CDP mouse events rather than a DOM click (#1483).
const POINTER = new Set(["hover", "dblclick", "back", "forward"]);
// `Meta+k` as the parameters of a CDP key event (the type is added by the caller), or
// null for a key this table does not know. A key with no modifier but Shift carries its
// text, so a bare `key:j` types into a focused field as well as reaching a key handler.
const keyPress = (combo) => {
  const parts = combo.split("+");
  const name = parts.pop();
  let modifiers = 0;
  for (const m of parts) {
    if (!(m in MODIFIERS)) return null;
    modifiers |= MODIFIERS[m];
  }
  let key, code, keyCode, text;
  if (name in NAMED_KEYS) {
    [key, code, keyCode, text] = NAMED_KEYS[name];
  } else if (name.length === 1) {
    key = modifiers & MODIFIERS.Shift ? name.toUpperCase() : name;
    const upper = name.toUpperCase();
    if (/[A-Z]/.test(upper)) [code, keyCode] = [`Key${upper}`, upper.charCodeAt(0)];
    else if (/[0-9]/.test(name)) [code, keyCode] = [`Digit${name}`, name.charCodeAt(0)];
    else if (name in PUNCTUATION) [code, keyCode] = PUNCTUATION[name];
    else return null;
    text = key;
  } else {
    return null;
  }
  const typing = (modifiers & ~MODIFIERS.Shift) === 0;
  return { key, code, windowsVirtualKeyCode: keyCode, modifiers, ...(typing && text ? { text, unmodifiedText: text } : {}) };
};
const flags = ["--headless=new", "--disable-gpu", "--hide-scrollbars", `--user-data-dir=${profile}`, `--remote-debugging-port=${debug}`, `--window-size=${size}`];
if (theme === "dark") flags.push("--blink-settings=preferredColorScheme=0");
if (theme === "light") flags.push("--blink-settings=preferredColorScheme=1");
const chromePath = findChrome();
if (!chromePath) {
  console.error("shot: no Chrome found (the macOS app, or google-chrome or chromium on PATH); set SHOT_CHROME");
  process.exit(1);
}
const chrome = spawn(chromePath, [...flags, "about:blank"], { stdio: "ignore", detached: true });
// Why Chrome is gone, once it is: an exit before it offered a page is a fault, not a slow start.
let chromeGone = null;
chrome.on("error", (e) => (chromeGone ??= `Chrome did not start (${e.message})`));
chrome.on("exit", (code, signal) => (chromeGone ??= `Chrome exited (${signal ? `signal ${signal}` : `code ${code}`})`));
// Chrome leads its own process group, so one SIGKILL reaches its helpers too; a plain
// SIGTERM to the main process left a browser running after a failing step (#1603).
// Every exit path goes through here: done(), a signal, the process's own exit.
const closeChrome = () => {
  try {
    process.kill(-chrome.pid, "SIGKILL");
  } catch {
    chrome.kill("SIGKILL");
  }
};
const done = (code, msg) => {
  if (msg) console.error(msg);
  closeChrome();
  process.exit(code);
};
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(sig, () => done(1, `shot: ${sig}`));
process.on("exit", closeChrome);
try {
  // What the DevTools endpoint last answered, for the failure when none is a page.
  let seen;
  const page = await poll(
    START_WAIT_S,
    async () => {
      seen = `:${debug} has not answered`;
      try {
        const targets = await (await fetch(`http://localhost:${debug}/json`, { signal: AbortSignal.timeout(10000) })).json();
        seen = `:${debug} listed ${targets.length} target(s)${targets.length ? `: ${targets.map((t) => t.type).join(", ")}` : ""}`;
        return targets.find((t) => t.type === "page");
      } catch (e) {
        seen = `:${debug} did not answer (${e.cause?.code ?? e.message})`;
      }
    },
    () => chromeGone,
  );
  if (!page && chromeGone) done(1, `shot: ${chromeGone} before it offered a page; last seen: ${seen}`);
  if (!page) done(1, `shot: Chrome offered no page within ${START_WAIT_S} s (SHOT_START_WAIT_S); last seen: ${seen}`);
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r, reject) => {
    ws.onopen = r;
    ws.onerror = () => reject(new Error(`the DevTools socket ${page.webSocketDebuggerUrl} would not open`));
  });
  // A connection lost mid-run leaves every pending command unanswered; fail naming it.
  ws.onclose = () => done(1, `shot: Chrome closed the DevTools connection${chromeGone ? ` (${chromeGone})` : ""}`);
  let seq = 0;
  const waiting = new Map();
  const uncaught = [];
  const consoleErrors = [];
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && waiting.has(m.id)) (waiting.get(m.id)(m), waiting.delete(m.id));
    if (m.method === "Runtime.exceptionThrown") {
      const d = m.params.exceptionDetails;
      uncaught.push((d.exception?.description ?? d.text ?? "an exception").split("\n")[0]);
    }
    if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") {
      consoleErrors.push(m.params.args.map((a) => a.value ?? a.description ?? "").join(" ").split("\n")[0]);
    }
  };
  // One DevTools command's answer; a Chrome that never answers it fails the shot.
  const send = async (method, params = {}) => {
    const id = ++seq;
    const answer = new Promise((resolve) => waiting.set(id, resolve));
    ws.send(JSON.stringify({ id, method, params }));
    const a = await bounded(START_WAIT_S * 1000, answer);
    if (!a) throw new Error(`Chrome did not answer ${method} within ${START_WAIT_S} s (SHOT_START_WAIT_S)`);
    return a;
  };
  const evaluate = async (expression) => {
    const a = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    return a.result?.result?.value;
  };
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await send("Runtime.enable");
  const base = `http://localhost:${port}/?fake-ipc${extra}`;
  // A statement that sets `el` to the element a click or pointer step names, or
  // leaves it undefined. `pointer` keeps only elements with a box (a pointer
  // cannot land on one without) and, failing a control, takes the innermost
  // element whose whole text is <name>, so a row or card that is no button is
  // reached through its key cell.
  const findJs = (name, pointer) => {
    const byId = name.startsWith("testid=");
    const want = JSON.stringify(byId ? name.slice(7) : name);
    return `
      const want = ${want};
      const enabled = (x) => !x.disabled && x.getAttribute("aria-disabled") !== "true";
      // A page keep-alive holds hidden (#2247) is never a step's target.
      const boxed = (x) => !x.closest(".tf-kept[hidden]") && (!${pointer} || x.getClientRects().length > 0);
      let el;
      if (${byId}) {
        el = [...document.querySelectorAll("[data-testid]")].find((x) => x.getAttribute("data-testid") === want && enabled(x) && boxed(x));
      } else {
        const all = [...document.querySelectorAll("button, a, summary, [role=button], [role=menuitem], [role=tab]")].filter((x) => enabled(x) && boxed(x));
        // The whole name wins; only when none matches does the leading label count
        // (an item with a dim description: <b>Import…</b><span>estate file, …</span>).
        el = all.find((x) => (x.getAttribute("aria-label") ?? x.textContent).trim() === want)
          ?? all.find((x) => !x.hasAttribute("aria-label") && x.firstElementChild?.textContent.trim() === want);
        if (!el && ${pointer}) {
          const whole = (x) => x.textContent.trim() === want;
          el = [...document.body.querySelectorAll("*")].find((x) => whole(x) && boxed(x) && ![...x.children].some(whole));
        }
      }`;
  };
  const clickJs = (name) => `(() => {${findJs(name, false)}
      if (!el) return false;
      el.click();
      return true;
    })()`;
  // The viewport point at the centre of the element a pointer step names, after
  // scrolling it into view; null when there is none.
  const pointJs = (name) => `(() => {${findJs(name, true)}
      if (!el) return null;
      el.scrollIntoView({ block: "nearest", inline: "nearest" });
      const r = el.getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    })()`;
  // CDP's mouse buttons and the `buttons` bit each holds down.
  const BUTTONS = { left: 1, back: 8, forward: 16 };
  const pointer = async (verb, { x, y }) => {
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    if (verb === "hover") return;
    const button = verb === "dblclick" ? "left" : verb;
    for (let clickCount = 1; clickCount <= (verb === "dblclick" ? 2 : 1); clickCount++) {
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button, buttons: BUTTONS[button], clickCount });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button, buttons: 0, clickCount });
    }
  };
  // The elements that scroll, the document's own scroller first.
  const scrollers = `() => [document.scrollingElement, ...document.querySelectorAll("*")].filter((x) => {
    if (!x || x.scrollHeight <= x.clientHeight + 1) return false;
    return x === document.scrollingElement || /auto|scroll/.test(getComputedStyle(x).overflowY);
  })`;
  const focusJs = (name) => {
    const byId = name.startsWith("testid=");
    const want = JSON.stringify(byId ? name.slice(7) : name);
    return `(() => {
      const want = ${want};
      const all = [...document.querySelectorAll("input, textarea, select, [contenteditable=true], [contenteditable='']")]
        .filter((x) => !x.disabled && x.getAttribute("aria-disabled") !== "true" && !x.closest(".tf-kept[hidden]"));
      const labelled = (x) => [x.getAttribute("aria-label"), x.getAttribute("placeholder"), ...[...(x.labels ?? [])].map((l) => l.textContent)]
        .some((text) => text?.trim() === want);
      const el = ${byId} ? all.find((x) => x.getAttribute("data-testid") === want) : all.find(labelled);
      if (!el) return false;
      el.focus();
      return true;
    })()`;
  };
  const selectJs = (arg) => {
    const byId = arg.startsWith("testid=");
    const rest = byId ? arg.slice(7) : arg;
    const at = rest.indexOf("=");
    const want = JSON.stringify(rest.slice(0, at));
    const option = JSON.stringify(rest.slice(at + 1));
    return `(() => {
      const want = ${want}, option = ${option};
      const all = [...document.querySelectorAll("select")].filter((x) => !x.disabled && !x.closest(".tf-kept[hidden]"));
      const labelled = (x) => [x.getAttribute("aria-label"), ...[...(x.labels ?? [])].map((l) => l.textContent)]
        .some((text) => text?.trim() === want);
      const el = ${byId} ? all.find((x) => x.getAttribute("data-testid") === want) : all.find(labelled);
      const pick = el && [...el.options].find((o) => o.textContent.trim() === option || o.value === option);
      if (!pick) return false;
      el.value = pick.value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return true;
    })()`;
  };
  const scrollJs = (arg) => {
    if (arg.startsWith("testid=")) {
      return `(() => {
        const el = [...document.querySelectorAll("[data-testid]")].find((x) => x.getAttribute("data-testid") === ${JSON.stringify(arg.slice(7))} && !x.closest(".tf-kept[hidden]"));
        if (!el) return false;
        el.scrollIntoView({ block: "start" });
        return true;
      })()`;
    }
    return `(() => {
      const largest = (${scrollers})().sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
      if (!largest) return false;
      largest.scrollBy(0, ${Number(arg)});
      return true;
    })()`;
  };
  const stepGuard = ` within ${STEP_WAIT_S} s (SHOT_STEP_WAIT_S)`;
  let first = true;
  for (const segment of spec.split(" > ")) {
    const [route, ...steps] = segment.split(" + ").map((s) => s.trim());
    const path = route.replace(/^#?\/?/, "");
    if (first) {
      await send("Page.navigate", { url: `${base}#/${path}` });
      // A dev server compiles the app on its first request, so the load is waited
      // for, not guessed; the pause after it is for the app's first render.
      let state = "unread";
      const loaded = await poll(START_WAIT_S, async () => (state = await evaluate("document.readyState")) === "complete");
      if (!loaded) done(1, `shot: #/${path} did not finish loading within ${START_WAIT_S} s (SHOT_START_WAIT_S); readyState: ${state}`);
      await sleep(4000);
      first = false;
    } else {
      await evaluate(`location.hash = ${JSON.stringify("#/" + path)}`);
      await sleep(1500);
    }
    for (const step of steps) {
      const colon = step.indexOf(":");
      const verb = colon < 0 ? step : step.slice(0, colon);
      const arg = step.slice(colon + 1);
      if (verb === "click" || verb === "focus") {
        const hit = await poll(STEP_WAIT_S, () => evaluate(verb === "click" ? clickJs(arg) : focusJs(arg)));
        if (!hit) done(1, `shot: step '${step}' found no enabled element on #/${path}${stepGuard}`);
      } else if (POINTER.has(verb)) {
        const at = await poll(STEP_WAIT_S, () => evaluate(pointJs(arg)));
        if (!at) done(1, `shot: step '${step}' found no visible enabled element on #/${path}${stepGuard}`);
        await pointer(verb, at);
      } else if (verb === "select" && arg.slice(arg.startsWith("testid=") ? 7 : 0).includes("=")) {
        const hit = await poll(STEP_WAIT_S, () => evaluate(selectJs(arg)));
        if (!hit) done(1, `shot: step '${step}' found no such select and option on #/${path}${stepGuard}`);
      } else if (verb === "type") {
        await send("Input.insertText", { text: arg });
      } else if (verb === "key") {
        const press = keyPress(arg);
        if (!press) done(1, `shot: step '${step}' names no key this script knows`);
        await send("Input.dispatchKeyEvent", { ...press, type: press.text ? "keyDown" : "rawKeyDown" });
        await send("Input.dispatchKeyEvent", { ...press, type: "keyUp", text: undefined });
      } else if (verb === "wait" && /^\d+$/.test(arg)) {
        await sleep(Number(arg));
      } else if (verb === "scroll" && (/^-?\d+$/.test(arg) || arg.startsWith("testid="))) {
        if (!(await evaluate(scrollJs(arg)))) done(1, `shot: step '${step}' found nothing to scroll on #/${path}`);
      } else {
        done(1, `shot: unknown step '${step}' (known: click:<name>, hover:<name>, dblclick:<name>, back:<name>, forward:<name>, focus:<name>, type:<text>, key:<combo>, select:<name>=<option>, wait:<ms>, scroll:<px>, scroll:testid=<id>)`);
      }
      await sleep(500);
    }
  }
  await sleep(600);
  if (process.env.SHOT_FULL === "1") {
    // Grown in turns: a taller viewport can lay out more (a virtual list), which
    // may overflow again.
    for (let i = 0, grown = height; i < 3; i++) {
      const overflow = await evaluate(`(${scrollers})().reduce((most, x) => Math.max(most, x.scrollHeight - x.clientHeight), 0)`);
      if (!(overflow > 0) || grown >= 8000) break;
      grown = Math.min(8000, grown + overflow);
      await send("Emulation.setDeviceMetricsOverride", { width, height: grown, deviceScaleFactor: 1, mobile: false });
      await sleep(800);
    }
  }
  const { result } = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(file, Buffer.from(result.data, "base64"));
  for (const text of consoleErrors) console.error(`shot: console.error ${text}`);
  for (const text of uncaught) console.error(`shot: uncaught ${text}`);
  if (uncaught.length > 0 && process.env.SHOT_ALLOW_ERRORS !== "1") {
    done(1, `shot: ${file} was written, but the page threw ${uncaught.length} uncaught exception(s); SHOT_ALLOW_ERRORS=1 lets it through`);
  }
  done(0);
} catch (e) {
  done(1, `shot: ${e.message}`);
}
