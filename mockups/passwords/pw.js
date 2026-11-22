/*
  The Passwords clickdummy: the Passwords page (the password role, drawn without naming a source) and
  Settings › Sources (where password managers are added like any other source). Reads and changes model.js.
*/
"use strict";
(() => {
  const { S, NOW, ADAPTERS, CAPS } = PW;
  const t0 = Date.now();
  const now = () => new Date(NOW.getTime() + (Date.now() - t0));
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const hm = (d) => d.toTimeString().slice(0, 5);
  const date = (d) => d.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
  const ago = (d) => { const n = PW.days(d, now()); return n <= 0 ? "today" : n === 1 ? "yesterday" : n < 60 ? `${n} days ago` : n < 365 ? `${Math.round(n / 30)} months ago` : "over a year ago"; };
  const plural = (n, w, ws = w + "s") => `${n} ${n === 1 ? w : ws}`;
  const stamp = () => `2026-11-17 ${hm(now())}`;

  const ICONS = {
    search: '<circle cx="7" cy="7" r="5"/><path d="M11 11l3.5 3.5"/>',
    plus: '<path d="M8 3v10M3 8h10"/>',
    copy: '<rect x="5.5" y="5.5" width="8" height="8" rx="1"/><path d="M10.5 3h-7a1 1 0 0 0-1 1v7"/>',
    eye: '<path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/>',
    eyeoff: '<path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z"/><path d="M2.5 2.5l11 11"/>',
    lock: '<rect x="3" y="7" width="10" height="7" rx="1"/><path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2"/>',
    unlock: '<rect x="3" y="7" width="10" height="7" rx="1"/><path d="M5.5 7V5a2.5 2.5 0 0 1 4.9-.7"/>',
    key: '<circle cx="5.5" cy="10.5" r="3"/><path d="M8 8.5l5.5-5.5M11 6l2 2"/>',
    warn: '<path d="M8 2l6.5 11.5h-13z"/><path d="M8 6.5v3M8 11.5v.01"/>',
    ext: '<path d="M9 2.5h4.5V7M13.5 2.5L7.5 8.5M12 9.5v3.5a.5.5 0 0 1-.5.5h-8a.5.5 0 0 1-.5-.5v-8a.5.5 0 0 1 .5-.5H7"/>',
    term: '<path d="M2.5 4l4 4-4 4M8.5 12.5h5"/>',
    refresh: '<path d="M13 8a5 5 0 1 1-1.6-3.7M13 2v3h-3"/>',
    pencil: '<path d="M10.5 2.5l3 3-8 8H2.5v-3z"/>',
    link: '<path d="M6.5 9.5l3-3M7 4.5l1.2-1.2a2.6 2.6 0 0 1 3.7 3.7L10.7 8.2M9 11.5l-1.2 1.2a2.6 2.6 0 0 1-3.7-3.7L5.3 7.8"/>',
    close: '<path d="M4 4l8 8M12 4l-8 8"/>',
    check: '<path d="M3 8.5l3.2 3L13 4.5"/>',
    gear: '<circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/>',
    clip: '<path d="M10.5 5l-4.6 4.6a1.4 1.4 0 0 0 2 2l5-5a2.8 2.8 0 0 0-4-4l-5 5a4.2 4.2 0 0 0 6 6l4-4"/>',
    clock: '<circle cx="8" cy="8" r="6"/><path d="M8 4.5V8l2.5 1.5"/>',
    shield: '<path d="M8 1.5l5.5 2v4c0 3.5-2.4 5.8-5.5 7-3.1-1.2-5.5-3.5-5.5-7v-4z"/>',
    left: '<path d="M10 3.5L5.5 8l4.5 4.5"/>',
    globe: '<circle cx="8" cy="8" r="6"/><path d="M2 8h12M8 2c1.8 1.7 2.6 3.7 2.6 6S9.8 12.3 8 14M8 2C6.2 3.7 5.4 5.7 5.4 8s.8 4.3 2.6 6"/>',
    chevron: '<path d="M5.5 3.5L10 8l-4.5 4.5"/>',
    folder: '<path d="M1.5 4.5a1 1 0 0 1 1-1h3.5l1.5 1.5h6a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z"/>',
    menu: '<path d="M2.5 4h11M2.5 8h11M2.5 12h11"/>',
    download: '<path d="M8 2.5v8M4.5 7L8 10.5 11.5 7M3 13.5h10"/>',
    VM: '<rect x="2" y="2.5" width="12" height="4.5" rx="1"/><rect x="2" y="9" width="12" height="4.5" rx="1"/><path d="M4.5 4.75h.01M4.5 11.25h.01"/>',
    Service: '<path d="M8 1.5l6 3v7l-6 3-6-3v-7z"/><path d="M2 4.5l6 3 6-3M8 7.5v7"/>',
    Database: '<ellipse cx="8" cy="3.8" rx="5" ry="1.8"/><path d="M3 3.8v8.4c0 1 2.2 1.8 5 1.8s5-.8 5-1.8V3.8M3 8c0 1 2.2 1.8 5 1.8S13 9 13 8"/>',
    "Reverse proxy": '<circle cx="8" cy="8" r="6"/><path d="M2 8h12M8 2c1.8 2 1.8 10 0 12M8 2c-1.8 2-1.8 10 0 12"/>',
    Flowrun: '<circle cx="3.5" cy="4" r="1.5"/><circle cx="12.5" cy="12" r="1.5"/><path d="M5 4h4a2 2 0 0 1 2 2v4.5"/>',
    source: '<path d="M6 1.5v3M10 1.5v3M4 4.5h8v3a4 4 0 0 1-8 0zM8 11.5v3"/>',
    err: '<circle cx="8" cy="8" r="6"/><path d="M6 6l4 4M10 6l-4 4"/>',
    ok: '<circle cx="8" cy="8" r="6"/><path d="M5.5 8.2l1.8 1.8 3.2-3.5"/>',
  };
  const icon = (n, cls = "s") => `<svg class="i ${cls}" viewBox="0 0 16 16" aria-hidden="true">${ICONS[n] || ICONS.key}</svg>`;
  const btn = (label, act, { ic, cls = "", sm = true, dis = "", data = "" } = {}) =>
    `<button type="button" class="tf-btn ${sm ? "tf-sm" : ""} ${cls}" data-do="${act}" ${data} ${dis ? `aria-disabled="true" title="${esc(dis)}"` : ""}>${ic ? icon(ic) : ""}${esc(label)}</button>`;
  const ibtn = (label, act, ic, data = "", cls = "") =>
    `<button type="button" class="tf-iconbtn tf-sm ${cls}" data-do="${act}" ${data} aria-label="${esc(label)}" title="${esc(label)}">${icon(ic)}</button>`;

  /* ---------- the mockup's switches ---------- */
  const params = new URLSearchParams(location.search);
  const SCEN = { today: "Today", two: "Pleasant and Bitwarden", comments: "Usage comments on", offline: "A source offline", empty: "No password sources" };
  const scenario = SCEN[params.get("scenario")] ? params.get("scenario") : "today";
  let theme = params.get("theme") || "";
  if (theme) document.documentElement.dataset.theme = theme;
  if (scenario === "comments") PW.source("team").reasonFolders = ["Tidewater/Payments/Prod"];
  if (scenario === "offline") Object.assign(PW.source("team"), { state: "offline", line: "Not answering since 10:21", since: "10:21" });
  if (scenario === "empty") S.sources = S.sources.filter((s) => !PW.isPasswords(s));
  if (scenario === "two") S.sources = S.sources.filter((s) => s.adapter !== "keepass");

  const U = { sel: { k: "all" }, q: "", cur: scenario === "empty" ? null : scenario === "offline" ? "pl-ops-mail" : "pl-payout-db", revealed: new Set(), fuse: null, fuseDone: 0, open: new Set() };
  const main = document.getElementById("main");
  const statusEl = document.getElementById("status");
  const entry = (id) => S.entries.find((e) => e.id === id);
  const visible = () => S.entries.filter(PW.visible);
  const pwSources = () => S.sources.filter(PW.isPasswords);
  const segs = (f) => f.split("/");
  const pathText = (f) => segs(f).join(" / ");
  /* With more than one password source, each source's folders sit under one top folder named after the source.
     That top folder is the only place the page names a source. With one source there is none. */
  const rootOf = (s) => (pwSources().length > 1 ? s.name + "/" : "");
  /* A source whose folders all sit under one root folder (Pleasant's Tidewater) skips it: that level says nothing. */
  const skip = (s) => {
    const fs = S.entries.filter((e) => e.src === s.id).map((e) => e.folder);
    const first = fs.length ? segs(fs[0])[0] : "";
    return first && fs.every((f) => f.startsWith(first + "/")) ? first + "/" : "";
  };
  /* a source's own folder path, as the tree draws it, and back */
  const show = (s, f) => { const k = skip(s); const rel = k && (f + "/").startsWith(k) ? f.slice(k.length) : f; return rel ? rootOf(s) + rel : rootOf(s).slice(0, -1); };
  const unshow = (s, p) => skip(s) + p.slice(rootOf(s).length);
  const place = (e) => show(PW.srcOf(e), e.folder);
  const sameSel = (a, b) => a.k === b.k && a.id === b.id && a.f === b.f;
  const selAttr = (s) => `data-k="${s.k}" ${s.id ? `data-id="${esc(s.id)}"` : ""} ${s.f ? `data-f="${esc(s.f)}"` : ""}`;
  const route = () => { const h = location.hash; if (h.startsWith("#/settings/sources/")) return { page: "source", id: decodeURIComponent(h.slice(19)) }; if (h.startsWith("#/settings")) return { page: "sources" }; return { page: "passwords" }; };

  /* ---------- the folder tree: one tree over every source's folders ---------- */
  function tree() {
    const root = { kids: new Map(), n: 0, path: "" };
    for (const e of visible()) {
      let node = root;
      segs(place(e)).forEach((name, i, all) => {
        if (!node.kids.has(name)) node.kids.set(name, { name, path: all.slice(0, i + 1).join("/"), kids: new Map(), n: 0 });
        node = node.kids.get(name);
        node.n++;
      });
    }
    return root;
  }
  const unfoldTo = (folder) => folder && segs(folder).forEach((_, i, all) => U.open.add(all.slice(0, i + 1).join("/")));
  /* where a folder's rule is set: only the folder that sets it is marked, the folders under it inherit */
  const commentAt = (path) => pwSources().some((s) => (s.reasonFolders || []).some((f) => show(s, f) === path));
  /* writes go to the source that holds the folder; the page never shows which one that is */
  const writeTarget = (path) => {
    const e = S.entries.find((x) => PW.visible(x) && PW.under(place(x), path));
    return { src: e.src, folder: unshow(PW.srcOf(e), path) };
  };

  function railItem(s, label, count, { cls = "", lead = "", tone = "" } = {}) {
    const cur = sameSel(U.sel, s) && !U.q;
    return `<button type="button" class="pw-ri ${cls}" data-do="sel" ${selAttr(s)} ${cur ? 'aria-current="true"' : ""}>${lead}<span class="lab">${esc(label)}</span>${count === "" ? "" : `<span class="tf-count ${tone}">${count}</span>`}</button>`;
  }
  function treeRows(node, depth) {
    const curFolder = entry(U.cur) && PW.visible(entry(U.cur)) ? place(entry(U.cur)) : null;
    let h = "";
    for (const k of [...node.kids.values()].sort((a, b) => a.name.localeCompare(b.name))) {
      const open = U.open.has(k.path);
      const s = { k: "folder", f: k.path };
      const cur = sameSel(U.sel, s) && !U.q;
      const holds = curFolder && PW.under(curFolder, k.path) && !(open && curFolder !== k.path);
      h += `<div class="pw-tn" style="--d:${depth}">
        ${k.kids.size ? `<button type="button" class="pw-fold" data-do="fold" data-f="${esc(k.path)}" aria-expanded="${open}" aria-label="${open ? "Fold" : "Unfold"} ${esc(k.name)}">${icon("chevron")}</button>` : `<span class="pw-fold"></span>`}
        <button type="button" class="pw-ri sub ${holds ? "holds" : ""}" data-do="sel" ${selAttr(s)} ${cur ? 'aria-current="true"' : ""} title="${esc(pathText(k.path))}">
          <span class="lab">${esc(k.name)}</span>${commentAt(k.path) ? `<span class="pw-asks" title="Requires a usage comment, here and in every folder under it">${icon("shield")}</span>` : ""}<span class="tf-count">${k.n}</span></button>
      </div>`;
      if (open && k.kids.size) h += treeRows(k, depth + 1);
    }
    return h;
  }
  function renderRail() {
    const vis = visible();
    const attn = vis.filter(PW.attention).length;
    let h = railItem({ k: "all" }, "All entries", vis.length, { lead: icon("key") });
    h += railItem({ k: "attention" }, "Needs attention", attn || "", { lead: icon("warn"), tone: attn ? "warn" : "", cls: attn ? "attn" : "" });
    h += `<div class="pw-rh">Folders</div>${treeRows(tree(), 0)}`;
    return h;
  }

  /* ---------- the list ---------- */
  function hit(e, q) {
    const hay = [e.title, e.user, e.url, place(e), e.notes].join(" ").toLowerCase();
    return q.toLowerCase().split(/\s+/).filter(Boolean).every((w) => hay.includes(w));
  }
  const hl = (text, q) => {
    let out = esc(text);
    for (const w of q.split(/\s+/).filter(Boolean)) out = out.replace(new RegExp(`(${w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "ig"), "<mark>$1</mark>");
    return out;
  };
  function listed() {
    const vis = visible();
    const byTitle = (a, b) => a.title.localeCompare(b.title);
    const byPlace = (a, b) => place(a).localeCompare(place(b)) || byTitle(a, b);
    if (U.q) return { title: `Matches for “${U.q}”`, rows: vis.filter((e) => hit(e, U.q)).sort(byTitle) };
    const s = U.sel;
    if (s.k === "attention") {
      const rank = { err: 0, warn: 1 };
      return { title: "Needs attention", sub: "Expiring soon, or unchanged for over 180 days.", rows: vis.filter(PW.attention).sort((a, b) => rank[PW.attention(a).tone] - rank[PW.attention(b).tone]) };
    }
    if (s.k === "folder") {
      const here = (e) => (place(e) === s.f ? 0 : 1);
      return { title: segs(s.f).at(-1), folder: s.f, sub: s.f.includes("/") ? pathText(s.f) : "", rows: vis.filter((e) => PW.under(place(e), s.f)).sort((a, b) => here(a) - here(b) || byPlace(a, b)), grouped: true, base: s.f };
    }
    return { title: "All entries", rows: vis.sort(byTitle) };
  }
  /* What an entry signs in to, read off its address: the tile on its row and on its page. */
  const kindOf = (e) =>
    /^(postgres|mysql|mariadb|redis|mongodb)/.test(e.url) ? { ic: "Database", t: "Database login" }
    : /^ssh:/.test(e.url) ? { ic: "term", t: "Server login" }
    : /^https?:/.test(e.url) ? { ic: "globe", t: "Web sign-in" }
    : e.files.length ? { ic: "clip", t: "Key with attachment" }
    : { ic: "key", t: "Secret" };
  const shortAge = (d) => { const n = PW.days(d, now()); return n < 1 ? "today" : n < 30 ? `${n} d` : n < 365 ? `${Math.round(n / 30)} mo` : `${Math.floor(n / 365)} yr`; };
  function row(e, q, where, sub = "") {
    const off = PW.srcOf(e).state === "offline";
    const a = PW.attention(e);
    const sec = e.user || e.url.replace(/^\w+:\/\//, "") || (e.files.length ? "key and attachment" : "secret only");
    const marks = [
      e.totp && PW.can(e, "totp") ? `<span title="Has a one-time code">${icon("clock")}</span>` : "",
      e.files.length ? `<span title="Has an attachment">${icon("clip")}</span>` : "",
      PW.needsReason(e) || e.approval ? `<span title="${e.approval ? "Needs approval" : "Requires a comment"}">${icon("shield")}</span>` : "",
    ].join("");
    return `<li><button type="button" class="pw-row ${off ? "is-off" : ""}" data-do="pick" data-e="${e.id}" aria-selected="${U.cur === e.id}">
      <span class="pw-kind">${icon(kindOf(e).ic)}</span><span class="t">${q ? hl(e.title, q) : esc(e.title)}</span>
      <span class="age" title="Changed ${esc(date(e.changed))}">${a ? `<span class="tf-dot tf-${a.tone}"></span>` : ""}${shortAge(e.changed)}</span>
      <span class="u">${q ? hl(sec, q) : esc(sec)}</span>
      <span class="r">${marks}</span>
      ${where ? `<span class="loc" title="${esc(pathText(place(e)))}">${q ? hl(pathText(place(e)), q) : esc(pathText(place(e)))}</span>` : sub ? `<span class="loc">${esc(sub)}</span>` : ""}
      ${a && U.sel.k === "attention" && !q ? `<span class="why ${a.tone}">${esc(a.text)}</span>` : ""}
    </button></li>`;
  }
  function renderList() {
    const L = listed();
    const off = pwSources().filter((s) => s.state === "offline");
    const offN = L.rows.filter((e) => off.some((s) => s.id === e.src)).length;
    const offBanner = offN ? `<div class="pw-banner err">${icon("err")}<span>${plural(offN, "entry", "entries")} here can't be read right now: their source has not answered since ${esc(off[0].since)}. Last synced at ${esc(off[0].listed)}.</span>${btn("Try again", "retry", { data: `data-s="${off[0].id}"` })}</div>` : "";
    const acts = btn("New entry", "new", { ic: "plus", cls: "tf-primary" });
    let rows = "", lastF = null;
    for (const e of L.rows) {
      const child = (x) => { const rel = place(x).slice(L.base.length + 1); return rel ? L.base + "/" + segs(rel)[0] : L.base; };
      if (L.grouped && child(e) !== lastF) {
        lastF = child(e);
        if (lastF !== L.base) rows += `<li><button type="button" class="pw-grp" data-do="sel" data-k="folder" data-f="${esc(lastF)}">${icon("folder")}${esc(segs(lastF).at(-1))}<span class="tf-count">${L.rows.filter((x) => child(x) === lastF).length}</span></button></li>`;
      }
      /* in a group, a row deeper than the group's folder says where under it */
      const deeper = L.grouped && place(e) !== lastF ? pathText(place(e).slice(lastF.length + 1)) : "";
      rows += row(e, U.q, !L.grouped, deeper);
    }
    if (!L.rows.length) {
      rows = `<li class="pw-none">${U.q ? `Nothing you can read matches “${esc(U.q)}”.`
        : U.sel.k === "attention" ? "Nothing is expiring or out of date. This list fills when something is."
        : "No entries here yet."}</li>`;
    }
    return `<div class="pw-lh">
        <label class="pw-search">${icon("search")}<input id="pw-q" type="search" placeholder="Search passwords" aria-label="Search passwords" value="${esc(U.q)}" spellcheck="false" autocomplete="off"><kbd>/</kbd></label>
        <div class="pw-lh-t">${ibtn("Folders", "rail", "menu", "", "pw-railbtn")}<h2>${esc(L.title)}</h2><span class="tf-count">${L.rows.length}</span><div class="tf-acts">${acts}</div></div>
        ${L.sub ? `<div class="sub">${esc(L.sub)}</div>` : ""}
      </div>
      <ul class="pw-rows" role="listbox" aria-label="Entries">${offBanner}${rows}</ul>`;
  }

  /* ---------- the entry ---------- */
  const cls = (c) => /[0-9]/.test(c) ? "d" : /[A-Z]/.test(c) ? "u" : /[a-z]/.test(c) ? "l" : "s";
  const strip = (pw, big) => `<span class="pw-sec show ${big ? "big" : ""}" aria-label="${esc(pw)}">${[...pw].map((c, i) => `<span class="${cls(c)}">${esc(c)}<i>${i + 1}</i></span>`).join("")}</span>`;
  const otp = (id) => { const slot = Math.floor(now() / 30000); let h = 7; for (const c of id + slot) h = (h * 31 + c.charCodeAt(0)) % 1000003; return String(h % 1000000).padStart(6, "0"); };
  const otpLeft = () => 30 - (Math.floor(now() / 1000) % 30);
  const ring = () => { const left = otpLeft(); return `<svg class="pw-ring ${left <= 5 ? "low" : ""}" viewBox="0 0 22 22" aria-hidden="true"><circle class="bg" cx="11" cy="11" r="9"/><circle class="fg" cx="11" cy="11" r="9" style="stroke-dashoffset:${(56.55 * (1 - left / 30)).toFixed(2)}"/></svg>`; };
  const offline = (e) => PW.srcOf(e).state === "offline" ? "Its source is not answering" : "";
  function why(e) {
    if (offline(e)) return offline(e);
    if (e.readonly) return `You can read ${pathText(place(e))}, not change it`;
    if (!PW.can(e, "write")) return "These entries can be read here, not changed";
    return "";
  }

  const initials = (name) => name.split(/\s+/).map((w) => w[0]).join("").slice(0, 2).toUpperCase();
  const ME = "Mara Lindqvist";
  function renderDetail() {
    if (!pwSources().length) return emptyPage();
    const e = entry(U.cur);
    if (!e || !PW.visible(e)) return `<div class="pw-empty"><h2>Pick an entry</h2><p>Search with <kbd>/</kbd>, move with the arrow keys, copy the password with <kbd>C</kbd> and the username with <kbd>U</kbd>.</p></div>`;
    const src = PW.srcOf(e);
    const a = PW.attention(e);
    const off = offline(e);
    const dis = why(e);
    const shown = U.revealed.has(e.id);
    const appr = e.approval && S.requested[e.id] !== "approved";
    const k = kindOf(e);

    let pwCell, pwActs;
    if (appr) {
      pwCell = S.requested[e.id] === "pending"
        ? `<span class="dim">You asked ${esc(e.approval)} at ${esc(S.requested.at)}. It shows once they approve.</span>`
        : `<span class="dim">Shown only after ${esc(e.approval)} approves.</span>`;
      pwActs = S.requested[e.id] === "pending" ? `<button type="button" class="mockonly" data-do="approve" data-e="${e.id}">Approve (mock)</button>` : btn("Ask for access", "ask", { ic: "key", dis: off });
    } else {
      pwCell = shown ? `<div>${strip(e.pw)}<div class="pw-key"><span>${e.pw.length} characters</span><span><b class="d">0-9</b> digits</span><span><b class="s">#!</b> symbols</span></div></div>`
        : `<span class="pw-slot" aria-label="Hidden">${"<i></i>".repeat(12)}</span>`;
      pwActs = (shown ? btn("Hide", "hide", { ic: "eyeoff" }) : btn("Show", "reveal", { ic: "eye", dis: off })) + btn("Copy", "copy-pw", { ic: "copy", cls: shown ? "" : "tf-primary", dis: off });
    }

    /* the sign-in panel: what someone came here for */
    const cr = [];
    const R = (label, val, acts = "", cls = "") => cr.push(`<div class="pw-cr ${cls}"><span class="l">${esc(label)}</span><span class="v">${val}</span><span class="tf-acts">${acts}</span></div>`);
    if (e.user) R("Username", `<span class="mono">${esc(e.user)}</span>`, btn("Copy", "copy-user", { ic: "copy" }));
    R(e.user ? "Password" : "Secret", pwCell, pwActs, "is-pw");
    if (e.totp && PW.can(e, "totp")) R("One-time code", `<span class="pw-otp" data-otp="${e.id}">${otp(e.id).replace(/(\d{3})/, "$1 ")}</span><span data-ring>${ring()}</span>`, btn("Copy", "copy-otp", { ic: "copy" }), off ? "is-offok" : "");
    if (e.url) {
      const web = /^https?:\/\//.test(e.url);
      R("Address", web ? `<a class="mono" href="#" data-do="open-url">${esc(e.url)}</a>` : `<span class="mono">${esc(e.url)}</span>`, web ? btn("Open", "open-url", { ic: "ext" }) : btn("Copy", "copy-url", { ic: "copy" }));
    }
    for (const [key, val] of e.fields) R(key, `<span class="mono">${esc(val)}</span>`, btn("Copy", "copy-field", { ic: "copy", data: `data-f="${esc(key)}"` }));

    /* notes and attachments */
    const files = PW.can(e, "attachments") ? e.files.map(([n, size]) => `<div class="pw-file">${icon("clip")}<span class="mono">${esc(n)}</span><span class="dim small">${esc(size)}</span>${btn("Save", "download", { ic: "download", dis: off, data: `data-f="${esc(n)}"` })}</div>`).join("") : "";
    const notesPanel = `<section class="tf-panel pw-pn"><header><h3>Notes</h3></header>
      <div class="pw-pb">${e.notes ? `<p class="pw-notes">${esc(e.notes)}</p>` : `<p class="dim">No notes. ${dis ? "" : `<button type="button" class="tf-linkish" data-do="edit">Add one</button>`}</p>`}${files}</div></section>`;

    /* details: where it lives, how old it is, the rules on it */
    const age = PW.days(e.changed, now());
    const ageTone = age > 180 ? "warn" : "";
    let facts = `<dt>Folder</dt><dd class="pw-path">${segs(place(e)).map((name, i, all) => `<a href="#" data-do="sel" data-k="folder" data-f="${esc(all.slice(0, i + 1).join("/"))}">${esc(name)}</a>`).join(`<span class="sep">/</span>`)}</dd>`;
    facts += `<dt>Changed</dt><dd><span>${date(e.changed)}, ${ago(e.changed)}</span><span class="pw-meter ${ageTone}" title="Entries unchanged for 180 days need attention"><i style="width:${Math.min(100, (age / 180) * 100).toFixed(0)}%"></i></span></dd>`;
    if (e.expires) {
      const n = PW.days(now(), e.expires);
      facts += `<dt>Expires</dt><dd><span>${date(e.expires)} <span class="tf-chip ${n <= 3 ? "err" : n <= 14 ? "warn" : ""}">in ${plural(n, "day")}</span></span>${e.expiresBy ? `<span class="dim small">${esc(e.expiresBy)}</span>` : ""}</dd>`;
    }
    if (PW.needsReason(e)) facts += `<dt>Use</dt><dd>A comment is asked for every show or copy</dd>`;
    if (e.approval) facts += `<dt>Access</dt><dd>${esc(e.approval)} approves who may read it</dd>`;
    if (e.readonly) facts += `<dt>Changes</dt><dd>You can read it, not change it</dd>`;
    facts += `<dt>Synced</dt><dd>${esc(src.listed)} today</dd>`;
    const aboutPanel = `<section class="tf-panel pw-pn"><header><h3>Details</h3></header><dl class="pw-facts">${facts}</dl></section>`;

    /* who did what, newest first, as a conversation reads */
    const hist = PW.can(e, "history") && e.history.length
      ? `<section class="tf-panel pw-pn pw-hs"><header><h3>History</h3><span class="tf-count">${e.history.length}</span></header><ol class="pw-hist">${e.history.map(([t, who, what, q]) =>
          `<li><span class="tf-av ${who === ME ? "tf-me" : ""}" aria-hidden="true">${esc(initials(who))}</span><div><p><b>${esc(who === ME ? "You" : who)}</b> ${esc(what)}<time>${esc(t)}</time></p>${q ? `<p class="q">${esc(q)}</p>` : ""}</div></li>`).join("")}</ol></section>` : "";

    const chips = [
      a ? `<span class="tf-chip ${a.tone}">${esc(a.text)}</span>` : "",
      e.readonly ? `<span class="tf-chip">Read only</span>` : "",
      PW.needsReason(e) ? `<span class="tf-chip info">${icon("shield")}Comment required</span>` : "",
      e.approval ? `<span class="tf-chip info">${icon("shield")}Needs approval</span>` : "",
    ].join("");
    const notes = off ? `<div class="pw-note err">${icon("err")}<span>This entry's source has not answered since ${esc(src.since)}. What you see is from the last sync at ${esc(src.listed)}; the password can't be read until it answers.${e.totp && PW.can(e, "totp") ? " The one-time code still works: it is made on this machine." : ""}</span><span class="tf-acts">${btn("Try again", "retry", { data: `data-s="${src.id}"` })}</span></div>` : "";

    return `<div class="pw-dw">
      ${btn("Back", "back", { ic: "left", cls: "tf-ghost pw-back" })}
      <header class="pw-head">
        <span class="pw-kind lg">${icon(k.ic, "")}</span>
        <div class="pw-ht"><h1>${esc(e.title)}</h1><div class="pw-chips"><span class="pw-kt">${esc(k.t)}</span>${chips}</div></div>
        <div class="tf-acts">${btn("Edit", "edit", { ic: "pencil", sm: false, dis })}${btn("Change password", "rotate", { ic: "refresh", sm: false, dis: dis || (appr ? "Ask for access first" : "") })}</div>
      </header>
      ${notes}
      <section class="tf-panel pw-cred" aria-label="Sign-in">${cr.join("")}</section>
      <div class="pw-two">${notesPanel}${aboutPanel}</div>
      ${hist}
      <p class="pw-prov">${icon("shield")}<span>The password is not in the mirror. The source's adapter reads it when you show or copy it.${e.totp && PW.can(e, "totp") ? " The one-time code is made on this machine from a key kept in the macOS keychain, so it works while the source is down." : ""}</span></p>
    </div>`;
  }
  function emptyPage() {
    return `<div class="pw-empty"><span class="pw-kind lg">${icon("key", "")}</span><h2>No passwords yet</h2><p>Passwords come from your sources, like tickets and pages do. Add a source that holds passwords, a KeePass file, a Pleasant Password Server or Bitwarden, and its entries show up here.</p>
      <p class="tf-acts pw-ea">${btn("Add a source", "add-source", { ic: "plus", cls: "tf-primary", sm: false })}${btn("Open Sources", "go-sources", { cls: "tf-ghost", sm: false })}</p></div>`;
  }

  /* ---------- Settings › Sources ---------- */
  const tone = (s) => (s.state === "ok" ? "ok" : s.state === "offline" ? "warn" : "err");
  function holdings(s) {
    if (!PW.isPasswords(s)) return s.count || "";
    const n = S.entries.filter((e) => e.src === s.id).length;
    return plural(n, "password");
  }
  function settingsNav(cur) {
    return `<nav class="st-nav" aria-label="Settings">${["Sources", "Checkouts and Open in…", "Time", "Desktop notifications", "Standup", "Monitoring", "Notes and capture", "Backup and export", "Appearance", "Performance", "Renderers", "Keyboard", "Logs and database"].map((n) => `<a href="${n === "Sources" ? "#/settings/sources" : "#"}" ${n === cur ? 'aria-current="page"' : ""}>${esc(n)}</a>`).join("")}</nav>`;
  }
  function renderSources() {
    return `${settingsNav("Sources")}<section class="st-main">
      <header class="st-h"><div><h1>Sources</h1><p>Where knobas reads from. Each source syncs on its own schedule; writes go back through the queue.</p></div>
        <div class="tf-acts">${btn("+ Add a source", "add-source", { cls: "tf-primary", sm: false })}${btn("Sync all", "sync-all", { sm: false })}</div></header>
      <div class="st-srcs">${S.sources.map((s) => `<a class="st-src" href="#/settings/sources/${encodeURIComponent(s.id)}">
          <span class="dot ${tone(s)}"></span>
          <span class="st-sn"><b>${esc(s.name)}</b><span class="dim">${esc(s.address)}</span></span>
          <span class="st-si">${esc(holdings(s))}</span>
          <span class="st-si ${s.state === "err" ? "st-err" : ""}">${esc(s.line)}</span>
          <span class="st-go" aria-hidden="true">›</span></a>`).join("")}</div>
    </section>`;
  }
  function renderSource(id) {
    const s = PW.source(id);
    if (!s) return `${settingsNav("Sources")}<section class="st-main"><p>That source is gone. <a href="#/settings/sources">Back to Sources</a></p></section>`;
    const A = PW.adapterOf(s);
    const pw = PW.isPasswords(s);
    const ent = S.entries.filter((e) => e.src === s.id);
    const nFolders = new Set(ent.map((e) => e.folder)).size;
    return `${settingsNav("Sources")}<section class="st-main">
      <div class="st-crumb"><a href="#/settings/sources">Sources</a><span class="sep">›</span><span>${esc(s.name)}</span></div>
      <header class="st-h"><div><h1>${esc(s.name)}</h1><p><span class="dot ${tone(s)}"></span> ${esc(s.line)}</p></div>
        <div class="tf-acts">
          ${btn("Sync now", "sync-one", { sm: false, data: `data-s="${s.id}"` })}
        </div></header>
      <dl class="st-facts">
        <dt>Kind</dt><dd>${esc(A.name)}</dd>
        <dt>${pw && s.adapter === "keepass" ? "File" : "Address"}</dt><dd class="mono">${esc(s.address)}</dd>
        ${s.who ? `<dt>Signed in as</dt><dd class="mono">${esc(s.who)}</dd>` : ""}
        <dt>Holds</dt><dd>${pw ? `${plural(ent.length, "password")} in ${plural(nFolders, "folder")}` : esc(s.count)}</dd>
        ${pw && A.caps.totp ? `<dt>One-time codes</dt><dd>${s.adapter === "pleasant" ? "Found on 1 entry, in the fields <span class=\"mono\">TOTP Seed</span> and <span class=\"mono\">TOTP Settings</span> (KeeTrayTOTP). The adapter reads every known plugin format and says here which one it found." : `Found on ${ent.filter((x) => x.totp).length} entries.`} The keys are kept in the keychain, not the mirror, and a code is made on this machine without asking the source.</dd>` : ""}
        ${pw ? `<dt>Syncs</dt><dd>${s.manual ? "Only when you press <i>Sync now</i>. Every sync reads each entry from the server, and the server may log that read." : "Every 15 min"}</dd>` : ""}
        ${pw ? `<dt>Credential</dt><dd>${esc(A.credential)} ${btn("Change", "change-cred", { cls: "tf-ghost", data: `data-s="${s.id}"` })}</dd>` : ""}
        ${pw && (s.reasonFolders || []).length ? `<dt>Comments</dt><dd>Required in ${s.reasonFolders.map((f) => `<span class="mono">${esc(pathText(f))}</span>`).join(", ")}, set on the server</dd>` : ""}
      </dl>
      ${pw ? `<h2 class="st-h2">Passwords</h2>
        <p class="st-p">This source fills the password role. The Passwords page draws what the adapter declares and leaves out the rest; it never says which source an entry came from.</p>
        <table class="capt">${CAPS.map(([c, l]) => `<tr><td>${esc(l)}</td><td class="${A.caps[c] ? "y" : "n"}">${A.caps[c] ? "Yes" : "No"}</td></tr>`).join("")}</table>` : ""}
      <div class="st-danger">${btn("Remove this source", "remove-source", { cls: "tf-danger", sm: false, data: `data-s="${s.id}"` })}<span class="dim small">${pw ? "Nothing changes in the password manager." : "Its synced items go; nothing changes at the source."}</span></div>
    </section>`;
  }

  /* ---------- render ---------- */
  /* The breadcrumb bar every full page has: where you are, then the role view's Sync now at its end. */
  function crumbs() {
    const e = entry(U.cur);
    const path = e && PW.visible(e) ? segs(place(e)) : U.sel.k === "folder" ? segs(U.sel.f) : [];
    const last = e && PW.visible(e) ? e.title : U.sel.k === "attention" ? "Needs attention" : "";
    const synced = pwSources().map((s) => s.listed).filter(Boolean).sort()[0];
    return `<nav class="tf-crumbs pw-crumbs" aria-label="Breadcrumb"><a class="tf-crumb" href="#" data-do="sel" data-k="all">Passwords</a>${path.map((name, i) => `<span class="sep">›</span><a class="tf-crumb" href="#" data-do="sel" data-k="folder" data-f="${esc(path.slice(0, i + 1).join("/"))}">${esc(name)}</a>`).join("")}${last ? `<span class="sep">›</span><strong>${esc(last)}</strong>` : ""}
      <span class="esc">${synced ? `Synced ${esc(synced)}` : ""}</span>${btn("Sync now", "sync-all", { ic: "refresh" })}</nav>`;
  }
  function render() {
    const r = route();
    document.querySelectorAll(".tf-nav a[data-page], .pw-settings").forEach((a) => {
      if (a.dataset.page === (r.page === "passwords" ? "passwords" : "settings")) a.setAttribute("aria-current", "page"); else a.removeAttribute("aria-current");
    });
    main.className = r.page === "passwords" ? (pwSources().length ? (U.showDetail ? "show-detail" : "") : "is-empty") : "is-settings";
    const q = document.activeElement?.id === "pw-q";
    if (r.page === "sources") main.innerHTML = renderSources();
    else if (r.page === "source") main.innerHTML = renderSource(r.id);
    else if (!pwSources().length) main.innerHTML = `<section class="pw-detail pw-full">${emptyPage()}</section>`;
    else main.innerHTML = `${crumbs()}<div class="pw-panes"><aside class="pw-rail" aria-label="Folders">${renderRail()}</aside><section class="pw-list">${renderList()}</section><section class="pw-detail" aria-label="Entry">${renderDetail()}</section></div>`;
    if (q) { const i = document.getElementById("pw-q"); i?.focus(); i?.setSelectionRange(i.value.length, i.value.length); }
    main.querySelector('.pw-row[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
    renderStatus();
  }
  function renderStatus() {
    const n = visible().length;
    let mid;
    if (U.fuse) {
      const left = Math.max(0, U.fuse.end - Date.now());
      mid = `<span class="pw-fuse"><span class="bar" style="--left:${(left / 200).toFixed(1)}%"></span>${icon("copy")}${esc(U.fuse.label)} is on the clipboard. Cleared in ${Math.ceil(left / 1000)} s</span> <button type="button" class="tf-btn tf-sm tf-ghost" data-do="clear">Clear now</button>`;
    } else if (U.fuseDone > Date.now()) mid = `<span class="pw-fuse done">${icon("check")}Clipboard cleared</span>`;
    else mid = "";
    const srcs = pwSources().map((s) => s.state === "offline" ? `<span class="bad">${esc(s.name)}: not answering since ${esc(s.since)}</span>` : `<span>${esc(s.name)} synced ${esc(s.listed || "")}</span>`).join("");
    statusEl.innerHTML = `<a class="srcs" href="#/settings/sources">Sources</a><span class="grow">${U.fuse || U.fuseDone > Date.now() ? mid : srcs}</span><span class="dim">${plural(n, "password")}</span>`;
  }

  /* ---------- toasts and dialogs ---------- */
  const toasts = document.createElement("div");
  toasts.className = "pw-toasts";
  document.body.append(toasts);
  function toast(msg, bad) {
    const t = document.createElement("div");
    t.className = `pw-toast ${bad ? "bad" : ""}`;
    t.setAttribute("role", "status");
    t.textContent = msg;
    toasts.append(t);
    setTimeout(() => t.remove(), 3400);
  }
  function dialog(title, body, wide) {
    const d = document.createElement("dialog");
    d.className = `dlg ${wide ? "wide" : ""}`;
    d.innerHTML = `<header><h2>${esc(title)}</h2><button type="button" class="tf-iconbtn" data-close aria-label="Close">${icon("close")}</button></header><div class="dlg-b">${body}</div>`;
    d.addEventListener("click", (ev) => { if (ev.target.closest("[data-close]")) d.close(); });
    d.addEventListener("close", () => d.remove());
    document.body.append(d);
    d.showModal();
    return d;
  }

  /* ---------- the gate in front of every password ---------- */
  function gate(e, verb, then, opts = {}) {
    if (offline(e) && !opts.offline) return toast("This entry's source is not answering. Try again once it is.", true);
    if (e.approval && S.requested[e.id] !== "approved") return askAccess(e);
    const did = verb === "Show password" ? "showed the password" : verb === "Copy password" ? "copied the password" : null;
    /* The usual case: no question. A source that records reads does so on its side, and the history shows it. */
    if (!PW.needsReason(e)) {
      if (did && PW.can(e, "audit")) e.history.unshift([stamp(), "Mara Lindqvist", did]);
      return then();
    }
    /* Only where the source requires a usage comment for the folder or entry. It wants one per use, so knobas asks every time and fills in the last one. */
    const job = S.job.key + " " + S.job.title;
    const d = dialog("Comment required", `
      <p class="dlg-p"><b>${esc(pathText(show(PW.srcOf(e), PW.reasonAt(e))))}</b> requires a usage comment, here and in every folder under it. The comment is kept in the entry's history with your name.</p>
      <div class="sf"><div class="fld"><label class="fld-l" for="rsn">Comment</label>
        <div class="sugg"><button type="button" class="tf-pill job" data-s="${esc(job)}">${esc(job)}</button><button type="button" class="tf-pill" data-s="On call">On call</button><button type="button" class="tf-pill" data-s="Incident">Incident</button></div>
        <textarea id="rsn" class="inp">${esc(S.granted[e.id]?.text || job)}</textarea>
        <span class="fld-h">${S.granted[e.id] ? `Your last comment for this entry, from ${esc(S.granted[e.id].at)}.` : "Filled in from the ticket you are working on."} Enter sends it.</span></div></div>
      <div class="dlg-f"><button type="button" class="tf-btn tf-ghost" data-close>Cancel</button><button type="button" class="tf-btn tf-primary" data-ok>${esc(verb)}</button></div>`);
    const ta = d.querySelector("#rsn");
    ta.focus();
    ta.select();
    const ok = () => {
      const text = ta.value.trim();
      if (!text) { ta.focus(); return; }
      S.granted[e.id] = { text, at: hm(now()) };
      e.history.unshift([stamp(), "Mara Lindqvist", did || `used the entry (${verb.toLowerCase()})`, text]);
      d.close();
      then();
    };
    ta.addEventListener("keydown", (ev) => { if (ev.key === "Enter" && !ev.shiftKey) { ev.preventDefault(); ok(); } });
    d.addEventListener("click", (ev) => {
      const s = ev.target.closest("[data-s]");
      if (s) { ta.value = s.dataset.s; ta.focus(); }
      if (ev.target.closest("[data-ok]")) ok();
    });
  }
  function askAccess(e) {
    const d = dialog("Ask for access", `
      <p class="dlg-p"><b>${esc(e.title)}</b> is shown only after ${esc(e.approval)} approves. knobas tells you in the Inbox when they answer.</p>
      <div class="sf"><div class="fld"><label class="fld-l" for="ask">Reason</label><textarea id="ask" class="inp">${esc(S.job.key + " " + S.job.title)}</textarea></div>
      <div class="fld"><label class="fld-l" for="dur">For how long</label><select id="dur" class="inp"><option>1 hour</option><option selected>Today</option><option>A week</option></select></div></div>
      <div class="dlg-f"><button type="button" class="tf-btn tf-ghost" data-close>Cancel</button><button type="button" class="tf-btn tf-primary" data-ok>Send request</button></div>`);
    d.querySelector("[data-ok]").addEventListener("click", () => {
      S.requested[e.id] = "pending";
      S.requested.at = hm(now());
      d.close();
      toast(`Asked ${e.approval} for access`);
      render();
    });
  }

  /* ---------- copying: the fuse ---------- */
  let fuseTimer = null;
  function copy(label) {
    U.fuse = { label, end: Date.now() + 20000 };
    clearInterval(fuseTimer);
    fuseTimer = setInterval(() => {
      if (U.fuse && U.fuse.end <= Date.now()) { U.fuse = null; U.fuseDone = Date.now() + 2500; }
      if (!U.fuse && U.fuseDone < Date.now()) clearInterval(fuseTimer);
      renderStatus();
    }, 250);
    renderStatus();
  }

  /* ---------- the generator ---------- */
  const WORDS = ["harbour", "pylon", "corvid", "fennel", "quay", "ledger", "bollard", "manifest", "berth", "winch", "keel", "hawser", "crane", "ferry", "pallet", "freight", "tide", "buoy", "anchor", "gantry"];
  function generate(o) {
    const rnd = (n) => crypto.getRandomValues(new Uint32Array(1))[0] % n;
    if (o.words) {
      const w = Array.from({ length: Math.max(3, Math.round(o.len / 6)) }, () => WORDS[rnd(WORDS.length)]);
      const i = rnd(w.length);
      w[i] = w[i].replace(/^./, (c) => c.toUpperCase());
      return w.join("-") + "-" + rnd(100);
    }
    let set = "abcdefghijkmnopqrstuvwxyz";
    if (o.upper) set += "ABCDEFGHJKLMNPQRSTUVWXYZ";
    if (o.digits) set += "23456789";
    if (o.symbols) set += "!#$%&*+-=?@^_";
    return Array.from({ length: o.len }, () => set[rnd(set.length)]).join("");
  }
  const genBlock = () => `<div class="gen"><div data-prev></div>
      <div class="gen-c"><div class="tf-seg" role="group" aria-label="Kind"><button type="button" data-kind="chars" aria-pressed="true">Characters</button><button type="button" data-kind="words" aria-pressed="false">Words</button></div>
        <label>Length <input type="range" min="12" max="48" value="20" data-len> <b data-lenv class="num">20</b></label>
        <label class="chk"><input type="checkbox" data-o="upper" checked>A-Z</label><label class="chk"><input type="checkbox" data-o="digits" checked>0-9</label><label class="chk"><input type="checkbox" data-o="symbols" checked>Symbols</label>
        <button type="button" class="tf-btn tf-sm tf-ghost" data-again>${icon("refresh")}Another</button></div></div>`;
  function wireGen(root, onChange) {
    const g = root.querySelector(".gen");
    const o = { len: 20, upper: true, digits: true, symbols: true, words: false };
    const roll = () => { const pw = generate(o); g.querySelector("[data-prev]").innerHTML = strip(pw, true); onChange(pw); };
    g.addEventListener("input", (ev) => {
      if (ev.target.dataset.len) { o.len = +ev.target.value; g.querySelector("[data-lenv]").textContent = o.len; }
      if (ev.target.dataset.o) o[ev.target.dataset.o] = ev.target.checked;
      roll();
    });
    g.addEventListener("click", (ev) => {
      const k = ev.target.closest("[data-kind]");
      if (k) { o.words = k.dataset.kind === "words"; g.querySelectorAll("[data-kind]").forEach((b) => b.setAttribute("aria-pressed", b === k)); roll(); }
      if (ev.target.closest("[data-again]")) roll();
    });
    roll();
  }

  /* ---------- entry dialogs ---------- */
  function rotate(e) {
    let pw = "";
    const host = e.url.replace(/^\w+:\/\//, "").replace(/[/:].*$/, "");
    const d = dialog("Change password", `
      <p class="dlg-p">A new password for <b>${esc(e.title)}</b>.${PW.can(e, "history") ? " The old one stays in the entry's history." : ""}</p>
      ${genBlock()}
      ${host ? `<div class="pw-note warn">${icon("warn")}<span>knobas changes the entry, not ${esc(host)}. Change it there first, then save it here.</span></div>` : ""}
      <div class="dlg-f"><button type="button" class="tf-btn tf-ghost" data-close>Cancel</button><button type="button" class="tf-btn tf-primary" data-ok>Save new password</button></div>`, true);
    wireGen(d, (p) => (pw = p));
    d.querySelector("[data-ok]").addEventListener("click", () => {
      e.pw = pw;
      e.changed = now();
      e.history.unshift([stamp(), "Mara Lindqvist", "changed the password"]);
      U.revealed.add(e.id);
      d.close();
      toast("Saved the new password");
      render();
    });
  }
  const sameSourceFolders = (src) => [...new Set(S.entries.filter((x) => x.src === src).map(place))].sort();
  function edit(e) {
    const d = dialog("Edit entry", `<div class="sf">
      <div class="fld"><label class="fld-l" for="et">Title</label><input id="et" class="inp" value="${esc(e.title)}"></div>
      <div class="fld"><label class="fld-l" for="ef">Folder</label><select id="ef" class="inp">${sameSourceFolders(e.src).map((f) => `<option value="${esc(f)}" ${f === place(e) ? "selected" : ""}>${esc(pathText(f))}</option>`).join("")}</select>
        <span class="fld-h">The folders it can move to. The new folder's rules apply, such as a required comment.</span></div>
      <div class="row2"><div class="fld"><label class="fld-l" for="eu">Username</label><input id="eu" class="inp mono" value="${esc(e.user)}"></div>
      <div class="fld"><label class="fld-l" for="ea">Address</label><input id="ea" class="inp mono" value="${esc(e.url)}"></div></div>
      <div class="fld"><label class="fld-l" for="en">Notes</label><textarea id="en" class="inp">${esc(e.notes)}</textarea></div></div>
      <div class="dlg-f"><button type="button" class="tf-btn tf-ghost" data-close>Cancel</button><button type="button" class="tf-btn tf-primary" data-ok>Save</button></div>`);
    d.querySelector("[data-ok]").addEventListener("click", () => {
      e.title = d.querySelector("#et").value.trim() || e.title;
      e.user = d.querySelector("#eu").value.trim();
      e.url = d.querySelector("#ea").value.trim();
      e.notes = d.querySelector("#en").value;
      const to = d.querySelector("#ef").value;
      if (to !== place(e)) { e.history.unshift([stamp(), "Mara Lindqvist", `moved it from ${pathText(place(e))}`]); e.folder = unshow(PW.srcOf(e), to); unfoldTo(to); }
      else e.history.unshift([stamp(), "Mara Lindqvist", "edited the entry"]);
      d.close();
      toast("Saved");
      render();
    });
  }
  function newEntry() {
    /* every folder a new entry may go in, from every source that takes writes; which source that is stays out of sight */
    const writable = [...new Set(visible().filter((e) => PW.can(e, "write") && !e.readonly && !offline(e)).map(place))].sort();
    if (!writable.length) return toast("No folder takes new entries right now", true);
    const pre = (U.sel.k === "folder" && writable.find((f) => PW.under(f, U.sel.f))) || writable[0];
    let pw = "";
    const d = dialog("New entry", `<div class="sf">
      <div class="fld"><label class="fld-l" for="nf">Folder</label><select id="nf" class="inp">${writable.map((f) => `<option value="${esc(f)}" ${f === pre ? "selected" : ""}>${esc(pathText(f))}</option>`).join("")}</select></div>
      <div class="fld"><label class="fld-l" for="nt">Title</label><input id="nt" class="inp" placeholder="vm-pay-02 root"></div>
      <div class="row2"><div class="fld"><label class="fld-l" for="nu">Username</label><input id="nu" class="inp mono"></div>
      <div class="fld"><label class="fld-l" for="na">Address</label><input id="na" class="inp mono" placeholder="ssh://10.20.4.12"></div></div>
      <div class="fld"><span class="fld-l">Password</span>${genBlock()}</div></div>
      <div class="dlg-f"><button type="button" class="tf-btn tf-ghost" data-close>Cancel</button><button type="button" class="tf-btn tf-primary" data-ok>Add entry</button></div>`, true);
    wireGen(d, (p) => (pw = p));
    d.querySelector("#nt").focus();
    d.querySelector("[data-ok]").addEventListener("click", () => {
      const title = d.querySelector("#nt").value.trim();
      if (!title) { d.querySelector("#nt").focus(); return; }
      const folder = d.querySelector("#nf").value;
      const e = { id: "n" + Date.now(), ...writeTarget(folder), title, user: d.querySelector("#nu").value.trim(), pw, url: d.querySelector("#na").value.trim(), notes: "", fields: [], files: [], totp: false, expires: null, readonly: false,
        changed: now(), history: [[stamp(), "Mara Lindqvist", "added the entry"]] };
      S.entries.push(e);
      U.cur = e.id;
      unfoldTo(folder);
      d.close();
      toast(`Added ${title} to ${pathText(folder)}`);
      render();
    });
  }

  /* ---------- source dialogs ---------- */
  /* Settings › Sources › a source › Credential › Change: the master password lives with the source, like any token. */
  function changeCred(s) {
    const A = PW.adapterOf(s);
    const d = dialog(`Change the ${A.secret.toLowerCase()}`, `
      <p class="dlg-p">For <b>${esc(s.name)}</b>. Change it in the password manager first; this only tells knobas the new one.</p>
      <div class="sf"><div class="fld"><label class="fld-l" for="cp">New ${esc(A.secret.toLowerCase())}</label><input id="cp" type="password" class="inp" autocomplete="off"><span class="fld-h">${esc(A.credential)}</span></div></div>
      <div class="dlg-f"><button type="button" class="tf-btn tf-ghost" data-close>Cancel</button><button type="button" class="tf-btn tf-primary" data-ok>Save and sync</button></div>`);
    d.querySelector("#cp").focus();
    d.querySelector("[data-ok]").addEventListener("click", () => { s.state = "ok"; s.line = "Synced just now"; s.listed = hm(now()); d.close(); toast(`Saved. ${s.name} synced.`); render(); });
  }
  /* Settings › Sources › Add a source, the app's dialog: pick what it is, fill in, test, add. */
  function addSource() {
    let ad = null, tested = false;
    const d = dialog("Add a source", "", true);
    const body = d.querySelector(".dlg-b");
    const draw = () => {
      const A = ad && ADAPTERS[ad];
      const pw = A && A.role === "passwords";
      body.innerHTML = `<div class="st-ph">What is it?</div>
        <div class="st-radio" role="radiogroup" aria-label="What is it?">${Object.entries(ADAPTERS).map(([k, X]) => `<label><input type="radio" name="ad" value="${k}" ${k === ad ? "checked" : ""}><span>${esc(X.name)}<small>${esc(X.holds)}</small></span></label>`).join("")}</div>
        <div class="st-form"><label for="inst">Install an adapter</label><div class="st-inst"><input id="inst" class="inp" placeholder="A repository URL, a manifest URL or a file"><button type="button" class="tf-btn" disabled>Install…</button></div></div>
        <p class="st-note">1Password, HashiCorp Vault or pass would come in this way: an external adapter that fills the password role.</p>
        ${A ? `<div class="st-form">
            <label for="f-addr">${esc(pw ? A.address.l : "Address")}</label><input id="f-addr" class="inp mono" placeholder="${esc(pw ? A.address.ph : "https://")}">
            ${(A.fields || []).map((f) => `<label for="f-${f.k}">${esc(f.l)}</label><input id="f-${f.k}" class="inp ${f.type ? "" : "mono"}" type="${f.type || "text"}" placeholder="${esc(f.ph)}">`).join("")}
            <label for="f-sec">${esc(pw ? A.secret : "Token")}</label><input id="f-sec" class="inp" type="password" autocomplete="off">
            <label for="f-name">Name</label><input id="f-name" class="inp" value="${esc(pw ? A.sample.name : A.name)}">
            <label for="f-id">Id</label><input id="f-id" class="inp mono" value="${esc((pw ? A.sample.name : A.name).toLowerCase().replace(/[^a-z0-9]+/g, "-"))}">
          </div>
          <p class="st-note">${pw ? esc(A.secretNote) + " " : ""}The id is permanent: everything from this source is named with it.</p>
          <p class="st-note ${tested ? "st-okt" : ""}" role="status">${tested ? `${icon("ok")} ${esc(pw ? A.report : "Signed in. Ready to sync")}` : "Test connection fills in what it can find."}</p>` : ""}
        <div class="dlg-f"><button type="button" class="tf-btn" data-test ${A ? "" : "disabled"}>Test connection</button><span class="spacer"></span><button type="button" class="tf-btn tf-ghost" data-close>Cancel</button><button type="button" class="tf-btn tf-primary" data-add ${tested ? "" : "disabled"}>Add source</button></div>`;
    };
    draw();
    body.addEventListener("change", (ev) => { if (ev.target.name === "ad") { ad = ev.target.value; tested = false; draw(); } });
    body.addEventListener("click", (ev) => {
      if (ev.target.closest("[data-test]") && ad) { tested = true; const keep = [...body.querySelectorAll(".st-form input")].map((i) => i.value); draw(); body.querySelectorAll(".st-form input").forEach((i, n) => { if (keep[n] !== undefined) i.value = keep[n]; }); }
      if (ev.target.closest("[data-add]") && tested) {
        const A = ADAPTERS[ad];
        const name = body.querySelector("#f-name").value.trim() || A.name;
        const id = body.querySelector("#f-id").value.trim() || ad + Date.now();
        const pw = A.role === "passwords";
        const s = { id, adapter: ad, name, address: body.querySelector("#f-addr").value.trim() || (pw ? A.sample.address : "example.internal"), state: "ok", line: "Synced just now", listed: hm(now()), count: pw ? "" : "Syncing…", who: pw && ad !== "keepass" ? "mara.lindqvist" : "", reasonFolders: [] };
        S.sources.push(s);
        if (pw) {
          const base = { user: "admin", url: "", notes: "", fields: [], files: [], totp: false, expires: null, readonly: false, changed: now(), history: [] };
          A.sample.folders.forEach((f, i) => {
            S.entries.push({ ...base, id: `${id}-${i}a`, src: id, folder: f, title: `${segs(f).at(-1)} router`, pw: generate({ len: 16, upper: 1, digits: 1, symbols: 1 }) });
            S.entries.push({ ...base, id: `${id}-${i}b`, src: id, folder: f, title: `${segs(f).at(-1)} backup user`, user: "backup", pw: generate({ len: 18, words: 1 }) });
            U.open.add(segs(rootOf(s) + f).slice(0, rootOf(s) ? 2 : 1).join("/")); if (rootOf(s)) U.open.add(s.name);
          });
        }
        d.close();
        toast(pw ? `Added ${name}. Its entries are on the Passwords page.` : `Added ${name}`);
        render();
      }
    });
  }

  /* ---------- actions ---------- */
  const ACT = {
    theme() { theme = (document.documentElement.dataset.theme === "dark" || (!theme && matchMedia("(prefers-color-scheme: dark)").matches)) ? "light" : "dark"; document.documentElement.dataset.theme = theme; history.replaceState(null, "", href({ theme })); render(); },
    timer() { toast(`Stops the timer on ${S.job.key} and opens the worklog draft`); },
    search() { document.getElementById("pw-q")?.focus(); },
    sel(b) { U.sel = { k: b.dataset.k, id: b.dataset.id, f: b.dataset.f }; U.q = ""; if (U.sel.f) unfoldTo(U.sel.f); document.body.classList.remove("rail-open"); const L = listed(); if (L.rows.length && !L.rows.some((e) => e.id === U.cur)) U.cur = L.rows[0].id; render(); },
    fold(b) { const k = b.dataset.f; U.open.has(k) ? U.open.delete(k) : U.open.add(k); render(); },
    pick(b) { U.cur = b.dataset.e; unfoldTo(place(entry(U.cur))); U.showDetail = true; render(); },
    back() { U.showDetail = false; render(); },
    rail() { document.body.classList.toggle("rail-open"); },
    reveal() { const e = entry(U.cur); gate(e, "Show password", () => { U.revealed.add(e.id); render(); }); },
    hide() { U.revealed.delete(U.cur); render(); },
    "copy-pw"() { const e = entry(U.cur); gate(e, "Copy password", () => { copy(`The password of ${e.title}`); render(); }); },
    "copy-user"() { const e = entry(U.cur); if (e.user) copy(`The username of ${e.title}`); },
    "copy-otp"() { const e = entry(U.cur); if (e.approval && S.requested[e.id] !== "approved") return askAccess(e); copy(`The code for ${e.title}`); render(); },
    "copy-field"(b) { copy(`${b.dataset.f} of ${entry(U.cur).title}`); },
    clear() { U.fuse = null; U.fuseDone = Date.now() + 2500; renderStatus(); },
    "open-url"() { toast(`Opens ${entry(U.cur).url} in the browser`); },
    "copy-url"() { copy(`The address of ${entry(U.cur).title}`); },
    download(b) { gate(entry(U.cur), "Save attachment", () => toast(`Saved ${b.dataset.f} to Downloads`)); },
    edit() { edit(entry(U.cur)); },
    rotate() { rotate(entry(U.cur)); },
    new() { newEntry(); },
    "change-cred"(b) { changeCred(PW.source(b.dataset.s)); },
    retry(b) { const s = PW.source(b.dataset.s); s.state = "ok"; s.listed = hm(now()); s.line = "Synced just now"; toast("The source answers again"); render(); },
    ask() { askAccess(entry(U.cur)); },
    approve(b) { S.requested[b.dataset.e] = "approved"; toast("Priya Nair approved your request until tonight."); render(); },
    "add-source"() { addSource(); },
    "go-sources"() { location.hash = "#/settings/sources"; },
    "sync-all"() { toast(`Syncing all ${S.sources.length} sources`); },
    "sync-one"(b) { toast(`Syncing ${PW.source(b.dataset.s).name}`); },
    "remove-source"(b) {
      if (!b.dataset.sure) { b.dataset.sure = "1"; b.lastChild.textContent = "Click again to remove"; return; }
      const s = PW.source(b.dataset.s);
      S.sources = S.sources.filter((x) => x !== s);
      S.entries = S.entries.filter((e) => e.src !== s.id);
      location.hash = "#/settings/sources";
      toast(`Removed ${s.name}`);
    },
  };
  document.addEventListener("click", (ev) => {
    const b = ev.target.closest("[data-do]");
    if (!b) return;
    if (b.getAttribute("aria-disabled") === "true") { if (b.title) toast(b.title, true); return; }
    const f = ACT[b.dataset.do];
    if (!f) return;
    ev.preventDefault();
    f(b);
  });
  main.addEventListener("input", (ev) => {
    if (ev.target.id !== "pw-q") return;
    U.q = ev.target.value.trim();
    const L = listed();
    if (L.rows.length && !L.rows.some((e) => e.id === U.cur)) U.cur = L.rows[0].id;
    render();
  });
  window.addEventListener("hashchange", render);
  document.addEventListener("keydown", (ev) => {
    if (route().page !== "passwords" || document.querySelector("dialog[open]") || ev.metaKey || ev.ctrlKey || ev.altKey) return;
    const typing = ev.target.closest?.("input, textarea, select");
    if (ev.target.id === "pw-q" && ev.key === "Escape") { U.q = ""; ev.target.value = ""; render(); return; }
    if (typing && !(ev.target.id === "pw-q" && (ev.key === "ArrowDown" || ev.key === "ArrowUp"))) return;
    if (ev.key === "/") { ev.preventDefault(); document.getElementById("pw-q")?.focus(); return; }
    if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
      const rows = listed().rows;
      if (!rows.length) return;
      ev.preventDefault();
      const i = rows.findIndex((e) => e.id === U.cur);
      U.cur = rows[Math.min(rows.length - 1, Math.max(0, i + (ev.key === "ArrowDown" ? 1 : -1)))].id;
      render();
      document.querySelector('.pw-row[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
      return;
    }
    if (ev.key === "c" && U.cur) ACT["copy-pw"]();
    if (ev.key === "u" && U.cur) ACT["copy-user"]();
  });
  setInterval(() => {
    document.querySelectorAll("[data-otp]").forEach((el) => { el.textContent = otp(el.dataset.otp).replace(/(\d{3})/, "$1 "); });
    document.querySelectorAll("[data-ring]").forEach((el) => { el.innerHTML = ring(); });
  }, 1000);

  /* ---------- the prototype switches ---------- */
  const proto = document.getElementById("proto");
  const href = (o) => { const p = new URLSearchParams(location.search); for (const [k, v] of Object.entries(o)) v ? p.set(k, v) : p.delete(k); return `?${p}${location.hash}`; };
  const isDark = () => document.documentElement.dataset.theme === "dark" || (!theme && matchMedia("(prefers-color-scheme: dark)").matches);
  const drawProto = () => {
    proto.innerHTML = `<div class="proto" role="group" aria-label="Prototype switches, not part of the app"><b>Mockup</b><span class="sep"></span>
      ${Object.entries(SCEN).map(([k, l]) => `<a href="${esc(href({ scenario: k === "today" ? "" : k }))}" class="${k === scenario ? "on" : ""}">${l}</a>`).join("")}
      <span class="sep"></span><button type="button" data-theme-next>${isDark() ? "Light" : "Dark"}</button></div>`;
  };
  proto.addEventListener("click", (ev) => {
    if (!ev.target.closest("[data-theme-next]")) return;
    theme = isDark() ? "light" : "dark";
    document.documentElement.dataset.theme = theme;
    history.replaceState(null, "", href({ theme }));
    drawProto();
  });
  window.addEventListener("hashchange", drawProto);
  drawProto();
  for (const e of visible()) { const p = segs(place(e)); U.open.add(p[0]); if (rootOf(PW.srcOf(e))) U.open.add(p.slice(0, 2).join("/")); }
  if (entry(U.cur)) unfoldTo(place(entry(U.cur)));
  render();
})();
