// The API explorer's layout, in headless Chrome against a built app under ?fake-ipc&fake-openapi (spec #2514: #2524's
// criteria, "single scrollbar gutter + matching response padding + styled headers + schema markdown table layout", and the
// answer pane #2533 rebuilt; first run from a scratch folder on #2524, kept here by the spec's whole-branch review).
//
// Usage:
//   testenv/ui-checks/build-app.sh . <folder>
//   testenv/ui-checks/with-built.sh <folder> <port> node testenv/ui-checks/api-explorer.mjs <out dir> --port <port>
//
// It opens the pet store's `GET /pets` and TeamCity's `properties` schema in the mock repository at the window's smallest
// (1100x600, two columns: the request under the page's *Request*), 1920x960 (three: the answer under the request) and
// 5120x1440 (four: the answer its own column), and fails when:
//   - a column holds a second scroller (`.tf-oas-col` inside `.tf-oas-col`), so the pane draws two gutters; the answer's
//     compact view, which scrolls inside its own box of held height, is left out;
//   - the answer's padding differs from the request's (16 px each side, 24 px under), or it carries a top rule in the four
//     columns, where it stands alone;
//   - the auth choice is not offered by hand (*Nothing*, *Basic*, *Bearer*, *API key*) for a spec that declares no scheme;
//   - the answer's headers are not one row each, 6 px above and below, a rule between rows;
//   - the answer's body bar (*Render as*, *Pipeline*, *Expand*, #2533) runs past its column, or wraps onto a second line in
//     three or four columns (at 1100x600 it wraps, *Pipeline* and *Expand* under the rest), or *Expand* stands alone on a
//     line, or the compact view's box is not drawn under it at a held height;
//   - *Expand* does not cover the explorer's pane exactly, with the view filling it to its foot and *Back to the explorer*
//     focused, or `Esc` does not give the pane back with the focus on *Expand*;
//   - a schema's *Part of* is not chips, 12 then all 23 behind *Show all*;
//   - a schema's Markdown table is held to a width (`max-width` on `.tf-oas-fd`), drawn over the small type, or does not
//     break a long word.
//
// Writes <out dir>/api-explorer.json and exits 1 on any miss, or if a step finds nothing.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launch, sleep } from "./chrome.mjs";

const args = process.argv.slice(2);
const out = args[0];
const port = Number(args[args.indexOf("--port") + 1]);
if (!out || !port || !args.includes("--port")) {
  console.error("usage: api-explorer.mjs <out dir> --port <port>");
  process.exit(2);
}
mkdirSync(out, { recursive: true });

const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";
const PETS = `${REPO}/at/get%20%2Fpets/api/openapi.yaml`;
const PROPERTIES = `${REPO}/at/schemas%2Fproperties/api/teamcity.json`;

const results = [];
const check = (name, ok, detail) => {
  results.push({ name, ok, detail });
  console.log(`${ok ? "ok  " : "MISS"} ${name}${ok ? "" : `: ${JSON.stringify(detail)}`}`);
};

const chrome = await launch("knobas-api-explorer");
const { send, evaluate } = chrome;
const json = async (expr) => JSON.parse(await evaluate(`JSON.stringify(${expr})`));
const waitFor = async (expr, ms = 15000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await evaluate(`Boolean(${expr})`).catch(() => false)) return true;
    await sleep(100);
  }
  return false;
};
const size = (width, height) => send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
const go = async (hash) => {
  await evaluate(`location.hash = ${JSON.stringify(hash)}`);
};
const press = (selector, words) =>
  evaluate(`(() => {
    const b = [...document.querySelectorAll(${JSON.stringify(selector)})].find((b) => b.textContent.trim().startsWith(${JSON.stringify(words)}));
    b?.click();
    return !!b;
  })()`);
const key = async (k) => {
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: k, code: k, windowsVirtualKeyCode: k === "Escape" ? 27 : 0 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code: k, windowsVirtualKeyCode: k === "Escape" ? 27 : 0 });
};

// The answer's padding and rule, the request's padding, and whether a column scrolls inside a column.
const COLUMNS = `(() => {
  const req = document.querySelector(".tf-oas-reqcol .tf-oas-req");
  const ans = document.querySelector(".tf-oas-reqcol .tf-oas-ans, .tf-oas-anscol > .tf-oas-ans");
  const r = req && getComputedStyle(req), a = ans && getComputedStyle(ans);
  return {
    cols: document.querySelector(".tf-oas")?.dataset.cols,
    // The answer's compact view scrolls inside its box of held height (#2533): that box is the view, not a second gutter.
    nested: [...document.querySelectorAll(".tf-oas-col .tf-oas-col")].some((c) => c.closest(".tf-oas-docbox") === null),
    reqScrolls: r?.overflowY,
    req: r && [r.paddingLeft, r.paddingRight, r.paddingBottom],
    ans: a && [a.paddingLeft, a.paddingRight, a.paddingBottom],
    ansRule: a?.borderTopWidth,
  };
})()`;

// The body bar's children on one line, the bar inside its column, and the compact view's box under it.
const BODYBAR = `(() => {
  const bar = [...document.querySelectorAll(".tf-oas-bodybar")].find((b) => !b.closest(".tf-oas-expanded"));
  if (!bar) return null;
  const kids = [...bar.children].filter((k) => k.getClientRects().length > 0).map((k) => k.getBoundingClientRect());
  const col = bar.closest(".tf-oas-col")?.getBoundingClientRect();
  const b = bar.getBoundingClientRect();
  const box = bar.parentElement.querySelector(":scope > .tf-oas-docbox");
  const bb = box?.getBoundingClientRect();
  return {
    kids: kids.length,
    // Centres, not tops: the bar centres a label and buttons of different heights on its line.
    mids: Math.round(Math.max(...kids.map((k) => k.top + k.height / 2)) - Math.min(...kids.map((k) => k.top + k.height / 2))),
    barH: Math.round(b.height),
    tallest: Math.round(Math.max(...kids.map((k) => k.height))),
    inCol: col ? b.left >= col.left - 0.5 && b.right <= col.right + 0.5 && Math.max(...kids.map((k) => k.right)) <= col.right + 0.5 : false,
    sideways: bar.scrollWidth > bar.clientWidth + 1,
    // *Expand* with no other control on its line: the bar wrapped between *Pipeline* and it.
    expandAlone: (() => {
      const buttons = [...bar.querySelectorAll("button")].filter((k) => k.getClientRects().length > 0);
      const e = buttons.find((k) => k.textContent.trim() === "Expand")?.getBoundingClientRect();
      if (!e) return null;
      const mid = e.top + e.height / 2;
      return !buttons.some((k) => {
        const r = k.getBoundingClientRect();
        return k.textContent.trim() !== "Expand" && Math.abs(r.top + r.height / 2 - mid) <= 2;
      });
    })(),
    box: bb ? { h: Math.round(bb.height), below: Math.round(bb.top - b.bottom), full: box.classList.contains("tf-oas-full") } : null,
  };
})()`;

// The expanded answer against the pane it covers.
const EXPANDED = `(() => {
  const pane = document.querySelector(".tf-oas")?.getBoundingClientRect();
  const ex = document.querySelector(".tf-oas-expanded");
  if (!pane || !ex) return null;
  const e = ex.getBoundingClientRect();
  const box = ex.querySelector(".tf-oas-docbox.tf-oas-full");
  const bb = box?.getBoundingClientRect();
  const off = (a, b) => Math.round(Math.abs(a - b));
  return {
    edges: [off(e.left, pane.left), off(e.top, pane.top), off(e.right, pane.right), off(e.bottom, pane.bottom)],
    boxToFoot: bb ? off(bb.bottom, e.bottom) : null,
    boxH: bb ? Math.round(bb.height) : null,
    focus: document.activeElement?.hasAttribute("data-oas-collapse") ?? false,
    inert: [...document.querySelector(".tf-oas").children].filter((c) => c !== ex && !c.hasAttribute("inert")).length,
  };
})()`;

// Send the shown request, open its answer's body, then measure the bar; Expand, measure, Esc back.
async function answerPane(label, { request = false, wraps = false } = {}) {
  if (request) {
    check(`${label}: the page offers Request`, await waitFor(`[...document.querySelectorAll(".tf-oas-pageseg button")].some((b) => b.textContent.trim() === "Request")`), null);
    await press(".tf-oas-pageseg button", "Request");
  }
  const sendable = await waitFor(`[...document.querySelectorAll("button")].some((b) => b.textContent.trim().startsWith("Send"))`);
  check(`${label}: a Send button`, sendable, null);
  await press("button", "Send");
  const bar = await waitFor(`[...document.querySelectorAll(".tf-oas-bodybar")].some((b) => !b.closest(".tf-oas-expanded")) && document.querySelector(".tf-oas-docbox:not([aria-busy])")`);
  check(`${label}: the answer's body bar and its view are drawn`, bar, null);
  if (!bar) return;
  await sleep(300);
  const m = await json(BODYBAR);
  const line = m !== null && m.mids <= 2 && m.barH <= m.tallest + 2;
  check(
    `${label}: the body bar is ${wraps ? "inside" : "one line inside"} its column, Expand not alone on a line, the compact view's box under it`,
    m !== null && m.kids >= 3 && (wraps || line) && m.expandAlone === false && m.inCol && !m.sideways && m.box !== null && !m.box.full && m.box.h >= 200 && m.box.below >= 0,
    m,
  );

  // Focused as a press focuses a button in Chromium and WebView2, so the way back has somewhere to go.
  const expand = await evaluate(`(() => {
    const b = [...document.querySelectorAll(".tf-oas-bodybar button")].find((b) => b.textContent.trim() === "Expand");
    b?.focus();
    b?.click();
    return !!b;
  })()`);
  check(`${label}: Expand is offered`, expand, null);
  const shown = await waitFor(`document.querySelector(".tf-oas-expanded .tf-oas-docbox.tf-oas-full:not([aria-busy])")`);
  check(`${label}: Expand draws the answer over the pane`, shown, null);
  if (!shown) return;
  await sleep(300);
  const e = await json(EXPANDED);
  check(
    `${label}: the expanded answer covers the pane, its view to the foot, Back focused, the rest inert`,
    e !== null && e.edges.every((d) => d <= 1) && e.boxToFoot !== null && e.boxToFoot <= 1 && e.boxH > 200 && e.focus && e.inert === 0,
    e,
  );
  await key("Escape");
  const back = await waitFor(`document.querySelector(".tf-oas-expanded") === null && document.activeElement?.textContent.trim() === "Expand"`, 4000);
  check(`${label}: Esc gives the pane back, focus on Expand`, back, await json(`({ expanded: document.querySelector(".tf-oas-expanded") !== null, focus: document.activeElement?.textContent.trim().slice(0, 40) })`));
}

try {
  await send("Page.enable");
  await send("Runtime.enable");
  await send("Emulation.setFocusEmulationEnabled", { enabled: true });
  await size(1920, 960);
  await send("Page.navigate", { url: `http://127.0.0.1:${port}/?fake-ipc&fake-openapi#/inbox` });
  check("the app loads", await waitFor(`document.readyState === "complete" && document.querySelector(".tf")`, 30000), null);
  await sleep(1500);

  // Three columns.
  await go(PETS);
  check("1920x960: the pet store's GET /pets opens", await waitFor(`document.querySelector('[id^="oas-scheme-"]')`), null);
  const c3 = await json(COLUMNS);
  check("1920x960: three columns", c3.cols === "3", c3);
  check("1920x960: one scroller per column", !c3.nested && c3.reqScrolls === "visible", c3);
  check(
    "1920x960: the answer's padding is the request's, a rule above it",
    JSON.stringify(c3.ans) === JSON.stringify(c3.req) && c3.ans?.[0] === "16px" && c3.ans?.[2] === "24px" && c3.ansRule === "1px",
    c3,
  );
  const auth = await json(`[...(document.querySelector('[id^="oas-scheme-"]')?.options ?? [])].map((o) => o.textContent.trim())`);
  check("1920x960: auth by hand, for a spec with no scheme", JSON.stringify(auth) === JSON.stringify(["Nothing", "Basic", "Bearer", "API key"]), auth);
  await answerPane("1920x960");
  check("1920x960: the answer offers Headers", await press(".tf-oas-ans button", "Headers "), null);
  const rows = await waitFor(`document.querySelectorAll(".tf-oas-anshdrs .tf-oas-anshdr").length >= 2`);
  const h = await json(`(() => {
    const rows = [...document.querySelectorAll(".tf-oas-anshdrs .tf-oas-anshdr")];
    return { rows: rows.length, rule: rows[1] && getComputedStyle(rows[1]).borderTopWidth, pad: rows[0] && [getComputedStyle(rows[0]).paddingTop, getComputedStyle(rows[0]).paddingBottom] };
  })()`);
  check("1920x960: the answer's headers, one row each, 6 px, a rule between", rows && h.rule === "1px" && JSON.stringify(h.pad) === '["6px","6px"]', h);

  // Four columns.
  await size(5120, 1440);
  check("5120x1440: four columns", await waitFor(`document.querySelector(".tf-oas")?.dataset.cols === "4"`), await json(COLUMNS));
  const c4 = await json(COLUMNS);
  check(
    "5120x1440: one scroller per column, the answer's padding the request's, no rule above it",
    !c4.nested && c4.ansRule === "0px" && c4.ans?.[0] === "16px" && c4.ans?.[1] === "16px" && c4.ans?.[2] === "24px",
    c4,
  );
  await answerPane("5120x1440");

  // Two columns: the window's smallest.
  await size(1100, 600);
  await go(`${REPO}/at/get%20%2Fpets%2F%7BpetId%7D/api/openapi.yaml`);
  await sleep(300);
  await go(PETS);
  check("1100x600: two columns", await waitFor(`document.querySelector(".tf-oas")?.dataset.cols === "2"`), await json(COLUMNS));
  // At the window's smallest the page's column is 456 px, and *Render as*, its three views, *Pipeline* and *Expand* need
  // about 440 px of a 408 px line: the bar wraps, *Pipeline* and *Expand* together under the rest (spec #2514's final
  // landing ruled this over *Expand* wrapping alone). Held to its column and to Expand's company, not to one line.
  await answerPane("1100x600", { request: true, wraps: true });

  // A schema's Part of, and its Markdown table.
  await size(1440, 900);
  await go(PROPERTIES);
  const twelve = await waitFor(`document.querySelectorAll(".tf-oas-chips .tf-oas-part").length === 12`, 20000);
  const before = await json(`(() => {
    const chips = [...document.querySelectorAll(".tf-oas-chips .tf-oas-part")];
    return { chips: chips.length, rule: chips[0] && getComputedStyle(chips[0]).borderTopWidth, more: [...document.querySelectorAll(".tf-oas-page button")].some((b) => b.textContent.trim() === "Show all") };
  })()`);
  await press(".tf-oas-page button", "Show all");
  const all = await waitFor(`document.querySelectorAll(".tf-oas-chips .tf-oas-part").length === 23`);
  const fewer = await json(`[...document.querySelectorAll(".tf-oas-page button")].some((b) => b.textContent.trim() === "Show fewer")`);
  check("1440x900: Part of is chips, 12, then 23 behind Show all", twelve && before.rule === "1px" && before.more && all && fewer, { before, all, fewer });
  const t = await json(`(() => {
    const host = document.querySelector(".tf-oas-page .tf-oas-measure");
    if (!host) return null;
    const fd = document.createElement("div");
    fd.className = "tf-oas-fd";
    fd.innerHTML = '<div class="tf-oas-prose"><div class="tf-md"><table><thead><tr><th>Status</th><th>Identifier</th><th>Description</th></tr></thead><tbody><tr><td><code>ACTIVE</code></td><td><code>settlement_window_open</code></td><td>The settlement window is currently accepting outbound instructions without manual review.</td></tr></tbody></table></div></div>';
    host.appendChild(fd);
    const table = fd.querySelector("table"), code = fd.querySelector("td code");
    const r = {
      maxWidth: getComputedStyle(fd).maxWidth,
      width: Math.round(table.getBoundingClientRect().width),
      type: parseFloat(getComputedStyle(table).fontSize),
      body: parseFloat(getComputedStyle(host).fontSize),
      wrap: getComputedStyle(table).overflowWrap,
      codeWrap: getComputedStyle(code).overflowWrap,
    };
    fd.remove();
    return r;
  })()`);
  check(
    "1440x900: a schema's Markdown table takes the width, small type, long words broken",
    t !== null && t.maxWidth === "none" && t.width > 500 && t.type < t.body && t.wrap === "break-word" && t.codeWrap === "break-word",
    t,
  );
} catch (e) {
  check("the run", false, String(e?.stack ?? e));
} finally {
  chrome.stop();
}

const missed = results.filter((r) => !r.ok);
writeFileSync(join(out, "api-explorer.json"), JSON.stringify({ results }, null, 2));
console.log(`\n${results.length - missed.length}/${results.length} checks hold`);
process.exit(missed.length === 0 ? 0 : 1);
