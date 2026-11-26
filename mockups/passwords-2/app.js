/*
  The Passwords clickdummy's page logic. One file, no build, no server: index.html loads data.js and this.
  Nothing leaves the page. The "clipboard" is a line in the status bar and never the real one, reads are
  timers, and a reload starts over. Today is Monday 2026-11-23, 10:24; Mara Lindqvist's timer runs on PAY-231.
*/
"use strict";
(() => {
  const { CAPS, ADAPTERS, TOTP_FORMATS, FILES } = DATA;
  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const clone = (o) => JSON.parse(JSON.stringify(o));
  const plural = (n, one, many = one + "s") => `${n} ${n === 1 ? one : many}`;
  const pad = (n) => String(n).padStart(2, "0");

  /* ------------------------------------------------------------------ the clock */
  const T0 = Date.now();
  const NOW0 = new Date("2026-11-23T10:24:07").getTime();
  const TODAY = "2026-11-23";
  const now = () => NOW0 + (Date.now() - T0);
  const hm = (t = now()) => { const d = new Date(t); return `${pad(d.getHours())}:${pad(d.getMinutes())}`; };
  const hms = (t = now()) => `${hm(t)}:${pad(new Date(t).getSeconds())}`;
  const ago = (at) => {
    if (!at) return "never";
    const [h, m] = at.split(":").map(Number), d = new Date(now());
    const mins = d.getHours() * 60 + d.getMinutes() - (h * 60 + m);
    return mins <= 0 ? "just now" : `${mins} min ago`;
  };
  const daysTo = (date) => Math.round((new Date(date + "T12:00:00") - new Date(TODAY + "T12:00:00")) / 864e5);

  const ME = { name: "Mara Lindqvist", login: "mara.lindqvist" };
  const JOB = { key: "PAY-231", title: "Retry failed SEPA payouts" };
  /* How long a shown password stays shown and a copied secret stays on the clipboard: the one-time code's own thirty seconds. */
  const HOLD = 30;
  const SECTIONS = [["sources", "Sources"], ["checkouts", "Checkouts and Open in…"], ["time", "Time"], ["notifications", "Desktop notifications"], ["standup", "Standup"], ["monitoring", "Monitoring"], ["notes", "Notes and capture"], ["backup", "Backup and export"], ["appearance", "Appearance"], ["performance", "Performance"], ["renderers", "Renderers"], ["keyboard", "Keyboard"], ["logs", "Logs and database"]];
  const INTERVALS = [20, 30, 45, 60, 300, 900, 1800, 3600, 0];
  const SCENARIOS = [["both", "Two sources"], ["comments", "Usage comments required"], ["offline", "One source offline"], ["one", "One source"], ["empty", "No password source"]];

  /* ------------------------------------------------------------------ the model and the view state */
  const params = new URLSearchParams(location.search);
  let scenario = SCENARIOS.some(([k]) => k === params.get("scenario")) ? params.get("scenario") : "both";
  let theme = ["light", "dark"].includes(params.get("theme")) ? params.get("theme") : "";
  let mkOpen = false;
  let M, V, R, TREE, dlg = null, newId = 0;

  function load() {
    const keep = scenario === "empty" ? [] : scenario === "one" ? ["team"] : ["team", "org"];
    M = {
      sources: clone(DATA.SOURCES).filter((s) => !ADAPTERS[s.adapter].role || keep.includes(s.id)),
      entries: clone(DATA.ENTRIES).filter((e) => keep.includes(e.src)),
    };
    const team = M.sources.find((s) => s.id === "team");
    if (team && scenario === "comments") team.commentFolders = [["Tidewater", "Payments", "Prod"]];
    if (team && scenario === "offline") { team.unreachable = true; team.down = "10:19"; }
    V = {
      folder: [], entry: null, q: "", open: new Set(), pane: "list", drawer: false,
      sec: {},        /* a secret's row state, by key: asking, reading, shown until, its last receipt */
      clip: null,     /* what the clipboard holds and until when */
      recent: ["pl-pg-stage", "bw-gitea", "pl-teamcity"],
      lastComment: "", log: {}, hist: {}, req: {}, approved: {}, sync: {}, test: {},
    };
  }

  const A = (s) => ADAPTERS[s.adapter];
  const src = (id) => M.sources.find((s) => s.id === id);
  const pwSources = () => M.sources.filter((s) => A(s).role);
  const multi = () => pwSources().length > 1;
  const cap = (s, k) => (A(s).caps || {})[k];
  const entriesOf = (s) => M.entries.filter((e) => e.src === s.id);
  const pathsOf = (s) => [...entriesOf(s).map((e) => e.path), ...(s.folders || [])];
  /* A source whose folders all share one root folder skips that root. */
  const skipN = (s) => { const p = pathsOf(s); return p.length > 0 && p.every((x) => x.length > 0 && x[0] === p[0][0]) ? 1 : 0; };
  /* With more than one password source, each source's folders sit under one top folder named after it. */
  const shown = (s, path) => [...(multi() ? [s.name] : []), ...path.slice(skipN(s))];
  const unshown = (s, segs) => { const own = segs.slice(multi() ? 1 : 0); return skipN(s) ? [pathsOf(s)[0][0], ...own] : own; };
  const pathOf = (e) => shown(src(e.src), e.path);
  const keyOf = (segs) => segs.map(encodeURIComponent).join("/");
  const segsOf = (k) => (k ? k.split("/").map(decodeURIComponent) : []);
  const under = (segs, base) => base.length <= segs.length && base.every((x, i) => segs[i] === x);
  const entry = (id) => M.entries.find((e) => e.id === id && src(e.src)?.synced);
  const visible = () => M.entries.filter((e) => src(e.src)?.synced);
  const sourceWord = (s) => (multi() ? s.name : "the password manager");
  const bare = (url) => String(url || "").replace(/^[a-z]+:\/\//i, "").replace(/\/$/, "");
  const isWeb = (url) => /^https?:\/\//i.test(url);
  const commentFolder = (e) => { const s = src(e.src); return cap(s, "comments") ? (s.commentFolders || []).find((f) => under(e.path, f)) : null; };
  const needsApproval = (e) => !!e.approver && !!cap(src(e.src), "requests");
  const isBlocked = (e) => needsApproval(e) && V.req[e.id]?.st !== "approved";
  const jobComment = () => `${JOB.key} ${JOB.title}`;
  const defaultComment = () => V.lastComment || jobComment();

  function buildTree() {
    const mk = (segs, s) => ({ segs, name: segs.length ? segs[segs.length - 1] : "All entries", kids: [], own: [], n: 0, src: s });
    const root = mk([], null);
    const at = (segs, s) => {
      let n = root;
      segs.forEach((name, i) => { let k = n.kids.find((x) => x.name === name); if (!k) { k = mk(segs.slice(0, i + 1), s); n.kids.push(k); } n = k; });
      return n;
    };
    for (const s of pwSources()) {
      if (multi()) at([s.name], s).top = true;
      if (!s.synced) continue;
      for (const e of entriesOf(s)) at(pathOf(e), s).own.push(e);
      for (const p of s.folders || []) at(shown(s, p), s);
    }
    const count = (n) => (n.n = n.own.length + n.kids.reduce((a, k) => a + count(k), 0));
    count(root);
    return root;
  }
  const nodeAt = (segs) => { let n = TREE; for (const name of segs) { n = n.kids.find((k) => k.name === name); if (!n) return null; } return n; };
  const groupsOf = (node) => { const out = []; const walk = (n) => { if (n.own.length) out.push(n); n.kids.forEach(walk); }; walk(node); return out; };
  const allNodes = (node = TREE, out = []) => { node.kids.forEach((k) => { out.push(k); allNodes(k, out); }); return out; };
  const srcOfNode = (n) => n.src || (pwSources().length === 1 ? pwSources()[0] : null);
  const commentAtNode = (n) => { const s = srcOfNode(n); return !!s && (s.commentFolders || []).some((f) => keyOf(shown(s, f)) === keyOf(n.segs)); };

  /* ------------------------------------------------------------------ small drawing helpers */
  const kbd = (k) => (k ? ` <kbd>${k}</kbd>` : "");
  function btn(label, act, { arg = "", cls = "tf-sm", k = "", title = "", dis = false } = {}) {
    return `<button type="button" class="tf-btn ${cls}" data-do="${act}" data-arg="${esc(arg)}" data-fk="${act}:${esc(arg)}"${title ? ` title="${esc(title)}"` : ""}${dis ? " disabled" : ""}>${label}${kbd(k)}</button>`;
  }
  const tint = (pw) => [...pw].map((c) => (/[0-9]/.test(c) ? `<span class="d">${c}</span>` : /[A-Za-z]/.test(c) ? esc(c) : `<span class="s">${esc(c)}</span>`)).join("");
  const hl = (text, q) => {
    let out = esc(text);
    for (const w of q.toLowerCase().split(/\s+/).filter(Boolean)) {
      out = out.replace(new RegExp(`(${w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})(?![^<]*>)`, "ig"), "<mark>$1</mark>");
    }
    return out;
  };
  function crumbs(parts, back = "", lead = "") {
    const trail = parts.map(([name, href], i) => (i === parts.length - 1 ? `<strong>${esc(name)}</strong>` : `<a class="tf-crumb" href="${href}">${esc(name)}</a><span class="tf-sep">›</span>`)).join("");
    return `<nav class="tf-crumbs" aria-label="Breadcrumb">${lead}${trail}${back ? `<span class="tf-esc">${esc(back)} <kbd>Esc</kbd></span>` : ""}</nav>`;
  }
  const toasts = $("#toasts");
  function toast(text, err = false) {
    const el = document.createElement("div");
    el.className = `tf-toast${err ? " tf-err" : ""}`;
    el.setAttribute("role", err ? "alert" : "status");
    el.textContent = text;
    toasts.append(el);
    setTimeout(() => el.remove(), err ? 6500 : 3800);
  }
  /* A redraw replaces markup; whatever had the focus gets it back by its `data-fk`. */
  function keepFocus(fn) {
    const a = document.activeElement, fk = a?.dataset?.fk;
    const sel = a && a.tagName === "INPUT" && a.type === "text" ? [a.selectionStart, a.selectionEnd] : null;
    fn();
    if (!fk) return;
    const b = $$("[data-fk]").find((x) => x.dataset.fk === fk);
    if (!b || b === document.activeElement) return;
    b.focus({ preventScroll: true });
    if (sel) try { b.setSelectionRange(sel[0], sel[1]); } catch { /* not a text field */ }
  }

  /* ------------------------------------------------------------------ one-time codes: made here, never read */
  const otpLeft = () => 30 - (Math.floor(now() / 1000) % 30);
  const otpSlot = () => Math.floor(now() / 30000);
  function otpCode(id) {
    let h = 2166136261;
    for (const c of `${id}:${otpSlot()}`) { h ^= c.charCodeAt(0); h = Math.imul(h, 16777619) >>> 0; }
    return String(h % 1000000).padStart(6, "0");
  }
  const codeHtml = (id) => { const c = otpCode(id); return `${c.slice(0, 3)}<i>${c.slice(3)}</i>`; };

  /* ------------------------------------------------------------------ the Passwords page */
  function listing() {
    if (V.q) {
      const words = V.q.toLowerCase().split(/\s+/).filter(Boolean);
      const hits = visible().filter((e) => { const hay = [e.title, e.user, e.url, pathOf(e).join(" ")].join(" ").toLowerCase(); return words.every((w) => hay.includes(w)); });
      return [{ kind: "hits", rows: hits }];
    }
    const node = nodeAt(V.folder) || TREE, out = [];
    if (node === TREE) {
      const recent = V.recent.map(entry).filter(Boolean);
      if (recent.length) out.push({ kind: "recent", rows: recent });
    }
    for (const g of groupsOf(node)) out.push({ kind: "folder", node: g, rows: g.own });
    return out;
  }
  const firstListed = () => listing()[0]?.rows[0] || null;

  function expiry(e) {
    if (!e.expires || !cap(src(e.src), "expiry")) return null;
    const n = daysTo(e.expires);
    return { n, tone: n <= 3 ? "err" : n <= 14 ? "warn" : "", text: n < 0 ? "Expired" : n === 0 ? "Expires today" : `Expires in ${plural(n, "day")}` };
  }

  function rowHtml(e, { path = false, q = "" } = {}) {
    const s = src(e.src), x = expiry(e);
    const sub = [
      e.user ? `<span class="tf-mono">${hl(e.user, q)}</span>` : "",
      path ? `<span>${hl(pathOf(e).join(" / "), q)}</span>` : e.url ? `<span class="tf-mono">${hl(bare(e.url), q)}</span>` : "",
    ].join("");
    const marks = [
      x && x.tone ? `<span class="tf-chip tf-${x.tone}">${x.text}</span>` : "",
      needsApproval(e) ? `<span class="tf-lab">approval</span>` : "",
      e.totp && cap(s, "totp") ? `<span class="tf-lab">code</span>` : "",
    ].join("");
    return `<button type="button" class="pw-row" ${e.id === V.entry ? 'aria-current="true"' : ""} data-do="open" data-arg="${e.id}" data-fk="row:${e.id}">
      <span class="pw-t">${hl(e.title, q)}</span><span class="pw-s">${sub || "&nbsp;"}</span>
      <span class="pw-e">${marks}<span class="tf-num" title="Changed ${e.changed}">${e.changed}</span></span></button>`;
  }

  const COMMENT_MARK = `<svg class="pw-mark" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" role="img" aria-label="Usage comment required"><title>A usage comment is required for every read here</title><path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z"/></svg>`;

  function rowsHtml() {
    const node = nodeAt(V.folder) || TREE;
    const parts = listing().map((g) => {
      if (g.kind === "hits") {
        if (!g.rows.length) return `<div class="tf-panel"><div class="tf-empty pw-blank"><p>Nothing matches "${esc(V.q)}". The filter looks at titles, usernames, addresses and folder names.</p>${btn("Clear the filter", "clearq")}</div></div>`;
        return `<section class="tf-panel pw-group"><header><h3>${plural(g.rows.length, "entry", "entries")} in every folder</h3></header>${g.rows.map((e) => rowHtml(e, { path: true, q: V.q })).join("")}</section>`;
      }
      if (g.kind === "recent") return `<section class="tf-panel pw-group"><header><h3>Used recently</h3></header>${g.rows.map((e) => rowHtml(e, { path: true })).join("")}</section>`;
      const rel = g.node.segs.slice(node.segs.length);
      const head = rel.length
        ? `<a href="#/passwords/in/${keyOf(g.node.segs)}">${rel.length > 1 ? `<span class="pw-pre">${esc(rel.slice(0, -1).join(" / "))} / </span>` : ""}${esc(rel[rel.length - 1])}</a>`
        : node.kids.length ? `Directly in ${esc(node.name)}` : esc(node.name);
      /* A long folder further down shows its first five; its own page shows all of it. */
      const cut = rel.length && g.rows.length > 6 && !g.rows.slice(5).some((e) => e.id === V.entry) ? 5 : g.rows.length;
      /* The tree's mark again, on every folder the rule reaches: it inherits down the tree. */
      const note = g.rows[0] && commentFolder(g.rows[0]) ? COMMENT_MARK : "";
      return `<section class="tf-panel pw-group"><header><h3>${head}</h3>${note}<span class="tf-count">${g.rows.length}</span></header>
        ${g.rows.slice(0, cut).map((e) => rowHtml(e)).join("")}${cut < g.rows.length ? `<button type="button" class="pw-more" data-do="folder" data-arg="${esc(keyOf(g.node.segs))}">Show all ${g.rows.length} in ${esc(g.node.name)}</button>` : ""}</section>`;
    });
    if (!parts.length) return `<div class="tf-panel"><div class="tf-empty pw-blank"><p>Nothing in ${esc(node.name)} yet.</p>${btn("New entry here", "new")}</div></div>`;
    return parts.join("");
  }

  function headHtml() {
    const node = nodeAt(V.folder) || TREE, root = node === TREE;
    const folders = allNodes(node).length;
    const sources = root ? pwSources() : [srcOfNode(node)].filter(Boolean);
    /* Entries stay listed with the time of the last sync, whatever the source's state. */
    const lines = sources.map((s) => {
      const name = multi() && root ? `${s.name} ` : "";
      if (!s.synced) return `${name || "It "}is not synced yet.`;
      const when = s.every === 0 ? `by hand at ${s.at}` : ago(s.at);
      if (s.down) return `${name || "It "}did not answer at ${s.down}; its entries are as synced ${when}.`;
      return `${name ? `${name}synced` : "Synced"} ${when}.`;
    });
    const count = V.q ? `${plural(listing()[0].rows.length, "entry matches", "entries match")} "${esc(V.q)}".`
      : `${plural(node.n, "entry", "entries")}${folders ? ` in ${plural(folders, "folder")}` : ""}.`;
    const busy = pwSources().some((s) => V.sync[s.id]);
    const canAdd = pwSources().some((s) => s.synced && cap(s, "write"));
    const own = !root && !V.q ? srcOfNode(node) : null;
    const rule = own && cap(own, "comments") && (own.commentFolders || []).some((f) => under(unshown(own, node.segs), f)) ? " A usage comment is required for every read here." : "";
    return `<header class="tf-fhead pw-head"><div><h1>${root ? "Passwords" : esc(node.name)}</h1><p class="tf-sub">${count} ${esc(lines.join(" "))}${rule}</p></div>
      <div class="tf-acts">${canAdd ? btn("New entry", "new", { cls: "", k: "N" }) : ""}
      <button type="button" class="tf-btn tf-syncnow" data-do="syncrole" data-fk="syncrole:" title="Sync every source this view is drawn from. It lists entries and reads no passwords."${busy ? " disabled" : ""}>${busy ? "Syncing…" : "Sync now"}</button></div></header>`;
  }

  function treeHtml() {
    const cur = keyOf(V.folder), open = V.entry && entry(V.entry) ? pathOf(entry(V.entry)) : null;
    const mark = COMMENT_MARK;
    const row = (n, d) => {
      const k = keyOf(n.segs), isOpen = V.open.has(k), has = n.kids.length > 0, here = k === cur && !V.q;
      const holds = !isOpen && has && open && under(open, n.segs) && !here;
      const state = n.top && n.src.down ? `<span class="tf-dot tf-err" role="img" aria-label="Not reachable" title="Did not answer at ${n.src.down}"></span>`
        : n.top && !n.src.synced ? `<span class="pw-n">not synced</span>` : "";
      return `<button type="button" role="treeitem" class="pw-node${n.top ? " pw-top" : ""}${holds ? " pw-holds" : ""}" style="--d:${d}" aria-level="${d + 1}"${has ? ` aria-expanded="${isOpen}"` : ""}${here ? ' aria-current="true"' : ""}
        data-do="folder" data-arg="${esc(k)}" data-fk="node:${esc(k)}" tabindex="${here ? 0 : -1}"><span class="pw-tw" data-twist>${has ? (isOpen ? "▾" : "▸") : ""}</span><span class="pw-nm">${esc(n.name)}</span>${commentAtNode(n) ? mark : ""}${state}${n.top && !n.src.synced ? "" : `<span class="pw-n">${n.n}</span>`}</button>${isOpen ? n.kids.map((c) => row(c, d + 1)).join("") : ""}`;
    };
    const atRoot = cur === "" && !V.q;
    return `<div class="tf-dl-body" role="tree" aria-label="Folders">
      <button type="button" role="treeitem" class="pw-node" style="--d:0" aria-level="1"${atRoot ? ' aria-current="true"' : ""} data-do="folder" data-arg="" data-fk="node:" tabindex="${atRoot || V.q ? 0 : -1}"><span class="pw-tw"></span><span class="pw-nm">All entries</span><span class="pw-n">${TREE.n}</span></button>
      ${TREE.kids.map((k) => row(k, 0)).join("")}</div>`;
  }

  /* ---- the entry ---- */
  const secretName = (k) => { const part = k.split("|")[1]; return part === "pw" ? "password" : part.startsWith("old") ? "earlier password" : part.startsWith("file") ? "attachment" : entry(k.split("|")[0]).fields[+part.slice(1)][0]; };
  function secretValue(k) {
    const [id, part] = k.split("|"), e = entry(id);
    if (part === "pw") return e.pw;
    if (part.startsWith("old")) return `${[...e.pw].reverse().join("").slice(0, 12)}-${part.slice(3)}`;
    return e.fields[+part.slice(1)][1];
  }

  function askForm(e, k, st) {
    const s = src(e.src), f = commentFolder(e), part = k.split("|")[1];
    const noun = part === "pw" ? " password" : part.startsWith("file") ? " file" : "";
    const verb = { show: "Show", copy: "Copy", open: "Open", save: "Save" }[st.ask] + noun;
    const val = st.draft ?? defaultComment();
    const other = val !== jobComment() ? ` <button type="button" class="tf-linkish" data-do="askuse" data-arg="${esc(k)}">Use ${esc(jobComment())}</button>` : "";
    return `<form class="pw-ask" data-form="ask" data-arg="${esc(k)}"><label for="ask-in">Usage comment</label>
      <input class="pw-in" id="ask-in" type="text" data-fk="ask:${esc(k)}" data-draft="${esc(k)}" value="${esc(val)}" autocomplete="off" required>
      <button class="tf-btn tf-primary" type="submit">${verb} <kbd>⏎</kbd></button>${btn("Cancel", "askcancel", { arg: k, cls: "tf-ghost" })}
      <p>${esc(shown(s, f).join(" / "))} requires one for every read. ${esc(sourceWord(s))[0].toUpperCase() + esc(sourceWord(s)).slice(1)} keeps it in its log, under ${ME.login}.${other}</p></form>`;
  }

  function secretHint(e, k, st, isShown, main) {
    const s = src(e.src), word = esc(sourceWord(s)), Word = word[0].toUpperCase() + word.slice(1);
    if (st.ask) return askForm(e, k, st);
    if (st.reading) return `<div class="pw-fh" role="status">Reading from ${word}…</div>`;
    const out = [];
    if (isShown) out.push(`<span class="pw-timeleft"><span class="pw-comb" data-comb="show:${esc(k)}"></span><span>Hides itself in <span data-left="show:${esc(k)}"></span>.</span></span>`);
    if (V.clip?.key === k) out.push(`On the clipboard. It clears in <span data-left="clip"></span>.`);
    if (st.receipt && (isShown || V.clip?.key === k) && cap(s, "audit")) out.push(`Read at ${st.receipt.at}. ${Word} recorded it under ${ME.login}${st.receipt.comment ? ", with your comment" : ""}.`);
    if (!out.length && main) {
      if (isBlocked(e)) out.push("Hidden until your request is approved.");
      else if (s.down) out.push(`Can't be read while ${word} does not answer.`);
      else if (commentFolder(e)) out.push(`Asks for a usage comment first: ${esc(shown(s, commentFolder(e)).join(" / "))} requires one.`);
      else if (cap(s, "audit")) out.push(`${Word} records each read under ${ME.login}.`);
    }
    return out.length ? `<div class="pw-fh">${out.join(" ")}</div>` : "";
  }

  function secretRow(e, k, label, main = false) {
    const s = src(e.src), st = V.sec[k] || {}, isShown = (st.shownUntil || 0) > now();
    const dis = !isShown && (!!s.down || isBlocked(e) || !!st.reading);
    const value = isShown ? `<span class="pw-clear">${tint(secretValue(k))}</span>` : `<span class="pw-cover" aria-label="Hidden">••••••••••••</span>`;
    const size = main ? "" : "tf-sm";
    const acts = btn(isShown ? "Hide" : "Show", isShown ? "hide" : "show", { arg: k, cls: size, k: main ? "S" : "", dis: !isShown && dis })
      + btn("Copy", "copy", { arg: k, cls: main ? "tf-primary" : size, k: main ? "C" : "", dis });
    return `<div class="pw-f${st.ask ? " pw-top" : ""}"><div class="pw-fl">${esc(label)}</div><div class="pw-fv">${value}</div><div class="pw-fa">${acts}</div>${secretHint(e, k, st, isShown, main)}</div>`;
  }

  function otpRow(e) {
    const s = src(e.src), blocked = isBlocked(e), copied = V.clip?.key === `${e.id}|otp`;
    const code = blocked ? `<span class="pw-code pw-cover" aria-label="Hidden">••• •••</span>` : `<span class="pw-code" data-otp="${e.id}">${codeHtml(e.id)}</span>
      <span class="pw-otpleft"><span class="pw-comb pw-lg" data-comb="otp"></span><span>New code in <span data-left="otp"></span></span></span>`;
    const hint = blocked ? "Hidden until your request is approved."
      : copied ? `On the clipboard. It clears in <span data-left="clip"></span>. Nothing was read from ${esc(sourceWord(s))}${commentFolder(e) ? ", so no comment was asked" : ""}.`
      : "Made on this Mac. Works offline.";
    return `<div class="pw-f pw-otp"><div class="pw-fl">One-time code</div><div class="pw-fv">${code}</div><div class="pw-fa">${btn("Copy", "copyotp", { arg: e.id, cls: "", k: "O", dis: blocked })}</div><div class="pw-fh">${hint}</div></div>`;
  }

  function gateHtml(e) {
    if (!needsApproval(e)) return "";
    const r = V.req[e.id] || { st: "none" }, s = src(e.src), who = esc(e.approver);
    if (r.st === "approved") return `<div class="pw-gate tf-ok" role="status">${who} approved your request. It holds until ${r.until}.</div>`;
    if (r.st === "pending") return `<div class="pw-gate" role="status"><span>Requested at ${r.at}. Waiting for ${who}.</span>
      <span class="tf-trail">${btn(r.checking ? "Checking…" : "Check now", "reqcheck", { arg: e.id, dis: !!r.checking || !!s.down })}</span>
      <p>${r.checked ? `Not approved yet when checked at ${r.checked}. ` : ""}knobas asks ${esc(sourceWord(s))} only when you press Check now.</p></div>`;
    if (r.st === "form") return `<div class="pw-gate"><span>${who} approves each use of this entry.</span>
      <form data-form="req" data-arg="${e.id}"><input class="pw-in" id="req-in" type="text" data-fk="req:${e.id}" aria-label="Why you need it" value="${esc(defaultComment())}" autocomplete="off" required>
      <button class="tf-btn tf-primary" type="submit">Send request <kbd>⏎</kbd></button>${btn("Cancel", "reqcancel", { arg: e.id, cls: "tf-ghost" })}</form>
      <p>${who} sees your name and this comment.</p></div>`;
    return `<div class="pw-gate"><span>${who} approves each use of this entry. Its password${e.totp ? " and its one-time code stay" : " stays"} hidden until then.</span>
      <span class="tf-trail">${btn("Request access…", "reqopen", { arg: e.id, dis: !!s.down })}</span></div>`;
  }

  function filesRow(e) {
    const s = src(e.src), word = esc(sourceWord(s));
    const rows = e.files.map((f, i) => {
      const k = `${e.id}|file${i}`, st = V.sec[k] || {}, dis = !!s.down || isBlocked(e) || !!st.reading;
      const under = st.ask ? askForm(e, k, st) : st.reading ? `<div class="pw-fh" role="status">Reading from ${word}…</div>` : st.note ? `<div class="pw-fh">${esc(st.note)}</div>` : "";
      return `<div class="pw-file"><span class="tf-tkkind" aria-hidden="true">${esc(f.kind)}</span><span><span class="tf-t">${esc(f.name)}</span><span class="tf-rsub">${esc(f.size)}</span></span>
        <span class="pw-fa">${btn("Open", "fileopen", { arg: k, dis })}${btn("Save…", "filesave", { arg: k, dis })}</span>${under}</div>`;
    }).join("");
    return `<div class="pw-f pw-top"><div class="pw-fl">Attachments</div><div class="pw-fv" style="grid-column: 2 / -1"><div class="pw-files">${rows}</div></div></div>`;
  }

  function historyHtml(e) {
    const s = src(e.src), kind = cap(s, "history");
    if (!kind) return "";
    const h = V.hist[e.id] || "closed", word = esc(sourceWord(s));
    let body = "";
    if (h === "reading") body = `<p class="pw-fh" style="padding: 0 16px 12px" role="status">Reading from ${word}…</p>`;
    if (h === "open" && kind === "log") {
      const rows = [...(V.log[e.id] || []).map((r) => [...r, true]), ...e.log];
      body = rows.length ? `<ul class="pw-log">${rows.map(([when, who, what, comment, mine]) => `<li><span class="pw-when">${esc(when)}</span><span><span class="${mine ? "pw-mine" : ""}">${esc(who)}</span> ${esc(what)}${comment ? ` <q>${esc(comment)}</q>` : ""}</span></li>`).join("")}</ul>`
        : `<p class="pw-fh" style="padding: 0 16px 12px">Nothing recorded for this entry yet.</p>`;
    }
    if (h === "open" && kind === "earlier") {
      body = e.earlier.length ? `<ul class="pw-log">${e.earlier.map((d, i) => `<li><span class="pw-when">until ${esc(d)}</span><span class="pw-cover" aria-label="Hidden">••••••••••••</span><span class="pw-fa">${btn("Copy", "copy", { arg: `${e.id}|old${i}` })}</span></li>`).join("")}</ul>`
        : `<p class="pw-fh" style="padding: 0 16px 12px">No earlier passwords are kept for this entry.</p>`;
    }
    const dim = h !== "closed" ? "" : s.down ? `can't be read while ${word} does not answer` : "read when you open it";
    return `<div class="pw-fold"><button type="button" class="pw-foldbtn" aria-expanded="${h !== "closed"}" data-do="hist" data-arg="${e.id}" data-fk="hist:${e.id}"${s.down && h === "closed" ? " disabled" : ""}><b>${kind === "log" ? "History" : "Earlier passwords"}</b><span class="tf-dim">${dim}</span></button>${body}</div>`;
  }

  function entryHtml(e) {
    const s = src(e.src), p = pathOf(e), x = expiry(e);
    const canWrite = cap(s, "write") && !e.readonly;
    const links = p.map((seg, i) => `<a href="#/passwords/in/${keyOf(p.slice(0, i + 1))}">${esc(seg)}</a>`).join(`<span class="pw-sl">/</span>`);
    const why = s.down ? `${s.name} does not answer` : "";
    const acts = canWrite ? `<div class="tf-acts">${btn("Change password…", "change", { arg: e.id, dis: !!s.down || isBlocked(e), title: why })}${btn("Edit", "edit", { arg: e.id, k: "E", dis: !!s.down, title: why })}</div>` : "";
    let h = `<header class="pw-ehead"><div class="pw-etitle"><div class="pw-epath">${links || "Not in a folder"}</div><h2>${esc(e.title)}</h2></div>${acts}</header>`;
    if (s.down) h += `<div class="tf-stale pw-enote" role="status"><span>${esc(sourceWord(s))[0].toUpperCase() + esc(sourceWord(s)).slice(1)} did not answer at ${s.down}. This entry is as listed at ${s.at}, and its password can't be read until it answers.</span>${btn(V.test[s.id] ? "Trying…" : "Try again", "retry", { arg: s.id, dis: !!V.test[s.id] })}</div>`;
    h += gateHtml(e);
    if (e.user) h += `<div class="pw-f"><div class="pw-fl">Username</div><div class="pw-fv tf-mono">${esc(e.user)}</div><div class="pw-fa">${btn("Copy", "copyplain", { arg: "username", k: "U" })}</div></div>`;
    h += secretRow(e, `${e.id}|pw`, "Password", true);
    if (e.totp && cap(s, "totp")) h += otpRow(e);
    if (e.url) h += `<div class="pw-f"><div class="pw-fl">Address</div><div class="pw-fv tf-mono">${esc(e.url)}</div><div class="pw-fa">${isWeb(e.url) ? btn("Open", "address", { arg: e.id, k: "A", title: "Opens in your browser" }) : btn("Copy", "address", { arg: e.id, k: "A" })}</div></div>`;
    if (x) h += `<div class="pw-f"><div class="pw-fl">Expires</div><div class="pw-fv">${esc(e.expires)} <span class="tf-chip${x.tone ? ` tf-${x.tone}` : ""}">${x.text.replace("Expires ", "")}</span></div></div>`;
    if (e.notes) h += `<div class="pw-f pw-top"><div class="pw-fl">Notes</div><div class="pw-fv pw-notes" style="grid-column: 2 / -1">${esc(e.notes)}</div></div>`;
    e.fields.forEach(([name, value, secret], i) => {
      h += secret ? secretRow(e, `${e.id}|f${i}`, name)
        : `<div class="pw-f"><div class="pw-fl">${esc(name)}</div><div class="pw-fv tf-mono">${esc(value)}</div><div class="pw-fa">${btn("Copy", "copyplain", { arg: name })}</div></div>`;
    });
    if (e.files.length && cap(s, "files")) h += filesRow(e);
    h += historyHtml(e);
    h += `<footer class="pw-efoot">Changed ${esc(e.changed)}${e.by ? ` by ${esc(e.by)}` : ""}. Listed at the last sync, ${s.every === 0 ? `by hand at ${s.at}` : ago(s.at)}.${e.readonly ? " You can read this entry but not change it." : ""}</footer>`;
    return h;
  }

  function entryColHtml() {
    const e = V.entry && entry(V.entry), node = nodeAt(V.folder) || TREE;
    if (!e) return `<div class="tf-panel"><div class="tf-empty pw-blank"><p>${V.q ? "No entry matches the filter." : "No entry is open."}</p></div></div>`;
    return `<button type="button" class="tf-linkish pw-back" data-do="back">‹ ${V.q ? "Matches" : esc(node === TREE ? "All entries" : node.name)}</button><article class="tf-panel pw-entry" aria-label="${esc(e.title)}">${entryHtml(e)}</article>`;
  }

  const filterHtml = () => `<label class="tf-fsearch pw-filter"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><circle cx="7" cy="7" r="5"/><path d="M11 11l3.5 3.5"/></svg>
    <input id="pw-q" type="text" data-fk="q" placeholder="Filter every folder" aria-label="Filter the entries of every folder" autocomplete="off" spellcheck="false" value="${esc(V.q)}"><kbd>/</kbd></label>`;
  const keysHtml = `<div class="pw-keys" aria-label="Keys"><span><kbd>↑</kbd> <kbd>↓</kbd> move</span><span><kbd>/</kbd> filter</span><span><kbd>C</kbd> copy the password</span><span><kbd>U</kbd> username</span><span><kbd>O</kbd> one-time code</span><span><kbd>Esc</kbd> up a folder</span></div>`;

  function passwordsPage() {
    const sources = pwSources();
    if (!sources.length) {
      return `<div class="tf-full pw-page">${crumbs([["Passwords"]])}<div class="pw-none"><div class="tf-empty"><h1>No password source yet</h1>
        <p>Add your password manager in Settings › Sources, the way Jira or Gitea is added. Its entries are listed here and found by the filter. A password stays where it is: knobas reads one only when you show or copy it.</p>
        <a class="tf-btn tf-primary" href="#/settings/sources" data-do="addpw">Add a password source</a></div></div></div>`;
    }
    if (sources.length === 1 && !sources[0].synced) {
      const s = sources[0], busy = !!V.sync[s.id];
      return `<div class="tf-full pw-page">${crumbs([["Passwords"]])}<div class="pw-none"><div class="tf-empty"><h1>${esc(s.name)} is not synced yet</h1>
        <p>${s.every === 0 ? "It syncs only when you press Sync now. " : ""}A sync lists its entries and their folders. It reads no passwords.</p>
        <button type="button" class="tf-btn tf-primary" data-do="syncrole"${busy ? " disabled" : ""}>${busy ? "Syncing…" : "Sync now"}</button></div></div></div>`;
    }
    const parts = [["Passwords", "#/passwords"], ...V.folder.map((seg, i) => [seg, `#/passwords/in/${keyOf(V.folder.slice(0, i + 1))}`])];
    const up = V.folder.length ? `Up to ${V.folder.length > 1 ? V.folder[V.folder.length - 2] : "Passwords"}` : "";
    const lead = `<button type="button" class="tf-btn tf-sm pw-foldersbtn" data-do="drawer" aria-expanded="${V.drawer}">Folders</button>`;
    return `<div class="tf-full pw-page">${crumbs(parts, up, lead)}
      <div class="pw-body${V.drawer ? " tf-rd-drawer" : ""}">
        <aside class="tf-rd-tree pw-tree" id="pw-tree">${treeHtml()}</aside>
        ${V.drawer ? `<div class="tf-rd-backdrop" data-do="drawer"></div>` : ""}
        <section class="pw-work" id="pw-work"><div class="pw-bench"><div id="pw-head">${headHtml()}</div>
          <div class="pw-cols" id="pw-cols" data-pane="${V.pane}">
            <div class="pw-listcol">${filterHtml()}<div class="pw-rows" id="pw-rows">${rowsHtml()}</div>${keysHtml}</div>
            <div class="pw-entrycol" id="pw-entrycol">${entryColHtml()}</div>
          </div></div></section>
      </div></div>`;
  }

  /* ------------------------------------------------------------------ Settings */
  const everyWords = (v) => (v === 0 ? "Only when I press Sync now" : `Every ${v % 3600 === 0 ? (v === 3600 ? "1 hour" : `${v / 3600} hours`) : v % 60 === 0 ? `${v / 60} min` : `${v} s`}`);
  const holdings = (s) => (A(s).role ? (s.synced ? plural(entriesOf(s).length, "entry", "entries") : "nothing listed yet") : s.holds);
  function sourceState(s) {
    if (V.sync[s.id]) return { tone: "run", text: "Syncing…" };
    if (s.down) return { tone: "err", text: `No answer at ${s.down}`, bad: true };
    if (s.synced === false) return { tone: "muted", text: "Not synced yet" };
    if (s.paused) return { tone: "muted", text: `Paused, last ${ago(s.at)}` };
    return { tone: "ok", text: s.every === 0 ? `Synced by hand at ${s.at}` : `Synced ${ago(s.at)}` };
  }

  function sourcesList() {
    return `<header class="tf-stg-h tf-stg-hline"><div><h1>Sources</h1><p>Where knobas reads from. Each source syncs on its own schedule; writes go back through the queue.</p></div>
      <div class="tf-acts"><button type="button" class="tf-btn tf-primary" data-do="addsource" data-fk="addsource:">+ Add a source</button><button type="button" class="tf-btn" data-do="syncall">Sync all</button></div></header>
      <div class="tf-stg-srcs">${M.sources.map((s) => { const st = sourceState(s); return `<a class="tf-stg-src" href="#/settings/source/${s.id}">
        <span class="tf-dot tf-${st.tone}"></span><span class="tf-stg-sn"><b>${esc(s.name)}</b><span class="tf-dim">${esc(bare(s.address) || "bitwarden.com")}</span></span>
        <span class="tf-stg-si">${esc(holdings(s))}</span><span class="tf-stg-si${st.bad ? " tf-stg-errt" : ""}">${esc(st.text)}</span><span class="tf-stg-go" aria-hidden="true">›</span></a>`; }).join("")}</div>`;
  }

  function capRows(s) {
    const a = A(s), n = entriesOf(s);
    const detail = {
      write: "A write is sent at once. A password never waits in the write queue.",
      totp: `${plural(n.filter((e) => e.totp).length, "entry has", "entries have")} a code. The keys are kept in macOS Keychain and never reach the mirror, so codes work while it is down.`,
      files: "Read when you open or save one.",
      history: a.caps.history === "log" ? "Who read and who changed an entry, read when you open it." : "An entry's last five passwords, read when you open them.",
      expiry: `${plural(n.filter((e) => e.expires).length, "entry has", "entries have")} a date.`,
      comments: (s.commentFolders || []).length ? `The server requires one under ${(s.commentFolders || []).map((f) => shown(s, f).join(" / ")).join(", ")}.` : "The server requires none in the folders you can read.",
      audit: `Every password read is recorded there under ${s.user}. A sync reads no passwords.`,
      requests: `${plural(n.filter((e) => e.approver).length, "entry needs", "entries need")} approval.`,
    };
    return CAPS.map(([k, label]) => `<div class="tf-stg-row"><div class="tf-stg-l">${label}</div><div class="tf-stg-c pw-capc">${a.caps[k]
      ? `<span class="pw-capy"><span class="tf-dot tf-ok"></span>Yes</span><span class="pw-capd">${esc(s.synced ? detail[k] : "")}</span>`
      : `<span class="pw-capn">Not offered</span>`}</div></div>`).join("");
  }

  function sourcePage(s) {
    const a = A(s), st = sourceState(s), run = V.sync[s.id], role = !!a.role;
    const line = `${a.name}, ${holdings(s)}. ${s.every === 0 ? "Syncs only when you press Sync now" : `Syncs ${everyWords(s.every).toLowerCase()}`}, last ${s.at ? (s.every === 0 ? `at ${s.at}` : ago(s.at)) : "never"}.`;
    const word = a.secret, reenter = role ? "Change…" : "Re-enter…";
    const test = V.test[s.id];
    let h = `<header class="tf-stg-h tf-stg-hline"><div><h1><span class="tf-dot tf-${st.tone}"></span> ${esc(s.name)}</h1><p>${esc(line)}</p></div>
      <div class="tf-acts"><button type="button" class="tf-btn" data-do="syncsource" data-arg="${s.id}" data-fk="syncsource:${s.id}"${run ? " disabled" : ""}>Sync now</button></div></header>`;
    if (run) h += `<div class="tf-stg-run" role="status"><progress max="100" value="0" data-run="${s.id}"></progress><p data-runwords="${s.id}">Starting…</p></div>`;
    if (s.down) h += `<div class="tf-stg-alert" role="alert"><span class="tf-dot tf-err"></span><span>No answer from ${esc(bare(s.address))} at ${s.down}.${role ? " Its entries stay listed; their passwords can't be read until it answers. One-time codes keep working." : ""}</span>${btn(test === "busy" ? "Trying…" : "Try again", "retry", { arg: s.id, cls: "tf-sm tf-primary", dis: test === "busy" })}</div>`;
    else if (role && !s.synced && !run) h += `<div class="tf-stg-alert" role="status"><span class="tf-dot tf-warn"></span><span>Not synced yet. Sync now lists its entries and their folders. It reads no passwords.</span>${btn("Sync now", "syncsource", { arg: s.id, cls: "tf-sm tf-primary" })}</div>`;
    h += `<section class="tf-stg-sec"><h2>Connection</h2>
      <div class="tf-stg-row"><label class="tf-stg-l" for="src-address">Address</label><div class="tf-stg-c"><input id="src-address" type="text" class="tf-stg-in" value="${esc(s.address)}"${a.role && !s.address ? ' placeholder="bitwarden.com"' : ""}></div></div>
      ${a.user ? `<div class="tf-stg-row"><label class="tf-stg-l" for="src-user">User</label><div class="tf-stg-c"><input id="src-user" type="text" class="tf-stg-in" value="${esc(s.user || "")}"></div></div>` : ""}
      ${s.adapter === "bitwarden" ? `<div class="tf-stg-row"><div class="tf-stg-l">API key<small>Signs in. It cannot decrypt anything.</small></div><div class="tf-stg-c tf-stg-inl"><span>Stored in macOS Keychain, for ${esc(s.user)}</span></div></div>` : ""}
      <div class="tf-stg-row"><div class="tf-stg-l">${word}${role ? `<small>${s.adapter === "bitwarden" ? "Decrypts the entries, on this Mac." : "Signs in at every sync and every read."}</small>` : ""}</div>
        <div class="tf-stg-c tf-stg-inl"><span>Stored in macOS Keychain</span>${btn(reenter, "cred", { arg: s.id })}</div></div>
      ${test && test !== "busy" ? `<div class="tf-stg-note${test.ok ? "" : " tf-stg-errt"}" role="status"><b>Found when tested:</b> ${esc(test.text)}</div>` : ""}
      <div class="tf-stg-acts">${btn(test === "busy" ? "Testing…" : "Test connection", "test", { arg: s.id, dis: test === "busy" })}<button type="button" class="tf-btn tf-sm tf-primary" disabled>Save</button></div></section>`;
    h += `<section class="tf-stg-sec"><h2>What it syncs</h2>
      <div class="tf-stg-row"><label class="tf-stg-l" for="src-every">Syncs</label><div class="tf-stg-c"><select id="src-every" class="tf-stg-in" data-change="every" data-arg="${s.id}" data-fk="every:${s.id}">${INTERVALS.map((v) => `<option value="${v}"${v === s.every ? " selected" : ""}>${everyWords(v)}</option>`).join("")}</select></div></div>
      ${s.every === 0 ? `<p class="tf-stg-note">Nothing is read from ${esc(s.name)} in the background. It syncs when you press Sync now, here${role ? " or on the Passwords page" : ""}.${role && a.caps.totp ? " One-time codes keep working between syncs." : ""}</p>` : ""}
      <div class="tf-stg-row"><div class="tf-stg-l">Syncing</div><div class="tf-stg-c"><button type="button" class="tf-stg-sw" role="switch" aria-checked="${!s.paused}" aria-label="Syncing" data-do="pause" data-arg="${s.id}" data-fk="pause:${s.id}"><i></i></button></div></div></section>`;
    if (role) {
      h += `<section class="tf-stg-sec"><h2>Passwords</h2><p class="tf-stg-p">What ${esc(s.name)} offers the Passwords page. A control is drawn there only where its source offers it. Entries sync into the mirror; a password or an attachment is read only when you show, copy or open it.</p>${capRows(s)}</section>`;
      if (s.adapter === "pleasant") {
        const found = s.synced ? s.totpFormat : null, codes = entriesOf(s).filter((e) => e.totp).length;
        h += `<section class="tf-stg-sec"><h2>Where the one-time code keys are</h2><p class="tf-stg-p">The server's codes come from a KeePass plugin that keeps each key in the entry's custom fields. The adapter reads every format below, moves the key to macOS Keychain and takes those fields out of the entry before it reaches the mirror. ${found ? "" : "It says which one it found after the first sync."}</p>
          <div class="pw-tblwrap"><table class="tf-tbl pw-formats"><thead><tr><th>Plugin</th><th>Field</th><th>Format</th><th>Found</th></tr></thead><tbody>${TOTP_FORMATS.map((f) => `<tr class="${f.id === found ? "pw-found" : ""}"><td>${esc(f.plugin)}</td><td><span class="tf-mono">${esc(f.field)}</span></td><td>${esc(f.format)}</td><td>${f.id === found ? `<span class="tf-chip tf-ok">${plural(codes, "entry", "entries")}</span>` : found ? `<span class="tf-dim">none</span>` : ""}</td></tr>`).join("")}</tbody></table></div></section>`;
      }
    }
    const runs = role ? s.runs || [] : [[`today ${s.at}`, "1.2 s", `${s.holds}, none changed.`]];
    h += `<section class="tf-stg-sec"><h2>Sync runs</h2>${runs.length ? `<table class="tf-tbl tf-stg-runs"><thead><tr><th>Started</th><th>Took</th><th>Result</th></tr></thead><tbody>${runs.slice(0, 8).map(([at, took, res]) => `<tr><td>${esc(at)}</td><td>${esc(took)}</td><td class="tf-stg-res">${esc(res)}</td></tr>`).join("")}</tbody></table>`
      : `<p class="tf-stg-p">No runs yet. The first one starts ${s.every === 0 ? "when you press" : "on the schedule, or with"} <b>Sync now</b>.</p>`}</section>`;
    h += `<section class="tf-stg-sec"><h2>Maintenance</h2>
      <div class="tf-stg-row"><div class="tf-stg-l">Fetch everything again<small>Reads the whole source from the top.${role ? " It still reads no passwords." : ""}</small></div><div class="tf-stg-c">${btn("Backfill", "syncsource", { arg: s.id, dis: !!run })}</div></div>
      <div class="tf-stg-row"><div class="tf-stg-l">Remove this source<small>Asks whether to keep what it synced.${role ? " Its credential and its code keys leave the keychain." : ""}</small></div><div class="tf-stg-c">${btn("Remove…", "remove", { arg: s.id, cls: "tf-sm tf-danger" })}</div></div></section>`;
    return h;
  }

  function settingsPage() {
    const s = R.source ? src(R.source) : null;
    const name = s ? s.name : R.source ? R.source : SECTIONS.find(([k]) => k === R.section)[1];
    const body = s ? sourcePage(s)
      : R.source ? `<header class="tf-stg-h"><h1>Sources</h1></header><p class="tf-stg-p">There is no source called ${esc(R.source)}. It may have been removed.</p>`
      : R.section === "sources" ? sourcesList()
      : `<header class="tf-stg-h"><h1>${esc(name)}</h1><p>This section is the app's as it is today, and not part of this mockup.</p></header>`;
    return `<div class="tf-full">${crumbs([["Settings", "#/settings"], [name]], s ? "Back to Sources" : "")}
      <div class="tf-stg"><nav class="tf-stg-nav" aria-label="Settings">${SECTIONS.map(([k, l]) => `<a href="#/settings/${k}"${k === R.section ? ' aria-current="page"' : ""}>${esc(l)}${k === "sources" && M.sources.some((x) => x.down) ? `<span class="tf-dot tf-err" role="img" aria-label="A source needs you" title="A source needs you"></span>` : ""}</a>`).join("")}</nav>
      <div class="tf-stg-body pw-stg-body">${body}</div></div></div>`;
  }

  const otherPage = (name) => `<div class="tf-full"><div class="pw-other"><div class="tf-empty"><p>${esc(name)} is the app's as it is today, and not part of this mockup.</p><a class="tf-btn" href="#/passwords">Back to Passwords</a></div></div></div>`;

  /* ------------------------------------------------------------------ the status bar: sources first, then the clipboard */
  function drawStatus() {
    const ok = M.sources.filter((s) => !s.down && s.synced !== false);
    const bad = M.sources.filter((s) => s.down);
    const clip = V.clip ? `<span class="pw-clip" role="status"><span class="pw-what">Clipboard: <b>${esc(V.clip.what)}</b> of ${esc(V.clip.title)}</span><span class="pw-comb pw-sm" data-comb="clip"></span><span data-left="clip" data-fmt="clock"></span><button type="button" data-do="clearclip">Clear now</button></span>` : "";
    $("#status").innerHTML = `<a class="tf-sources" href="#/settings/sources">Sources</a>
      <span class="tf-healthy">${ok.map((s) => `<span>${esc(s.name)} ${s.every === 0 ? `${s.at}, by hand` : s.at}</span>`).join("")}</span>
      ${bad.map((s) => `<span class="tf-bad">${esc(s.name)}: no answer at ${s.down} <a href="#/settings/source/${s.id}">Open its page</a></span>`).join("")}
      <span class="tf-queue">${clip}</span>`;
    paint();
  }

  /* ------------------------------------------------------------------ drawing */
  const main = $("#main");
  function render() {
    TREE = buildTree();
    const work = $("#pw-work"), top = work ? work.scrollTop : 0;
    keepFocus(() => {
      $$(".tf-nav a").forEach((a) => (a.dataset.page === R.page || a.dataset.page === R.name ? a.setAttribute("aria-current", "page") : a.removeAttribute("aria-current")));
      R.page === "settings" ? $("#settingsbtn").setAttribute("aria-current", "page") : $("#settingsbtn").removeAttribute("aria-current");
      main.innerHTML = R.page === "passwords" ? passwordsPage() : R.page === "settings" ? settingsPage() : otherPage(R.name);
      drawStatus();
    });
    if (top && $("#pw-work")) $("#pw-work").scrollTop = top;
    drawMk();
    tick();
  }
  const part = (id, html) => { const el = $(id); if (el) keepFocus(() => { el.innerHTML = html(); }); paint(); };
  const drawEntry = () => part("#pw-entrycol", entryColHtml);
  const drawTree = () => part("#pw-tree", treeHtml);
  function drawList() {
    part("#pw-rows", rowsHtml);
    part("#pw-head", headHtml);
    part("#pw-tree", treeHtml);
    drawEntry();
  }

  /* Whatever counts down is redrawn in place four times a second: no transition, so it works under reduced motion. */
  let lastSlot = -1;
  function left(kind) {
    if (kind === "otp") return otpLeft();
    if (kind === "clip") return V.clip ? Math.max(0, Math.ceil((V.clip.until - now()) / 1000)) : 0;
    const st = V.sec[kind.slice(5)];
    return st?.shownUntil ? Math.max(0, Math.ceil((st.shownUntil - now()) / 1000)) : 0;
  }
  function tick() {
    let stale = false;
    for (const k of Object.keys(V.sec)) { const st = V.sec[k]; if (st.shownUntil && st.shownUntil <= now()) { st.shownUntil = 0; stale = true; } }
    if (V.clip && V.clip.until <= now()) { V.clip = null; toast("Clipboard cleared."); stale = true; drawStatus(); }
    /* Never under someone typing a comment: the redraw waits for the next tick. */
    if (stale && !document.activeElement?.matches?.("#pw-entrycol input")) keepFocus(() => { const el = $("#pw-entrycol"); if (el) el.innerHTML = entryColHtml(); });
    paint();
  }
  /* The countdowns alone, so a redraw can fill its own at once. */
  function paint() {
    for (const el of $$("[data-comb]")) { const n = left(el.dataset.comb); el.style.setProperty("--n", n); el.classList.toggle("pw-low", n <= 5); el.setAttribute("role", "img"); el.setAttribute("aria-label", `${n} seconds left`); }
    for (const el of $$("[data-left]")) { const n = left(el.dataset.left); el.textContent = el.dataset.fmt === "clock" ? `0:${pad(n)}` : `${n} s`; }
    if (otpSlot() !== lastSlot) { lastSlot = otpSlot(); for (const el of $$("[data-otp]")) el.innerHTML = codeHtml(el.dataset.otp); }
    for (const id of Object.keys(V.sync)) {
      const run = V.sync[id], f = Math.min(1, (Date.now() - run.t0) / run.ms), s = src(id), bar = $(`[data-run="${id}"]`), words = $(`[data-runwords="${id}"]`);
      if (bar) bar.value = Math.round(f * 100);
      if (words && s) words.textContent = A(s).role ? `Listing folders and entries: ${Math.round(f * entriesOf(s).length)} of ${entriesOf(s).length} entries, ${((Date.now() - run.t0) / 1000).toFixed(1)} s. No passwords are read.` : `Reading what changed, ${((Date.now() - run.t0) / 1000).toFixed(1)} s.`;
    }
    const t = $("#timer-t"); if (t) { const mins = 72 + Math.floor((Date.now() - T0) / 60000); t.textContent = `${Math.floor(mins / 60)}:${pad(mins % 60)}`; }
  }

  /* ------------------------------------------------------------------ reads: the one path every secret takes */
  const touch = (e) => { V.recent = [e.id, ...V.recent.filter((id) => id !== e.id)].slice(0, 4); };
  function setClip(e, key, what) { V.clip = { key, what, title: e.title, until: now() + HOLD * 1000 }; }

  /* Offline and an open request stop a read before it starts; a folder that requires a usage comment asks for it in
     the row; then the adapter reads, which is slow on one source and quick on the other. */
  function read(e, k, verb, done) {
    const s = src(e.src), st = (V.sec[k] ||= {});
    if (s.down || isBlocked(e) || st.reading) return;
    if (commentFolder(e) && st.comment == null) {
      st.ask = verb; st.pending = done; st.draft = null;
      drawEntry();
      const input = $("#ask-in"); input?.focus(); input?.select();
      return;
    }
    st.ask = null; st.reading = verb;
    drawEntry();
    setTimeout(() => {
      if (V.sec[k] !== st) return;
      st.reading = null;
      st.receipt = { at: hms(), comment: st.comment || "" };
      if (cap(s, "audit")) (V.log[e.id] ||= []).unshift([`today ${hm()}`, "You", done.words, st.comment || ""]);
      st.comment = null;
      touch(e);
      done.run(st);
      drawEntry(); drawStatus();
      if (!V.q && !V.folder.length) part("#pw-rows", rowsHtml);
    }, A(s).latency);
  }

  /* ------------------------------------------------------------------ syncing */
  function syncSource(s, quiet = false) {
    if (V.sync[s.id]) return;
    const ms = A(s).role ? A(s).latency * 3.2 : 900;
    V.sync[s.id] = { t0: Date.now(), ms };
    setTimeout(() => {
      if (!M.sources.includes(s)) return;
      delete V.sync[s.id];
      if (s.unreachable) {
        s.down = hm();
        toast(`Could not sync ${s.name}: no answer from ${bare(s.address)}.`, true);
      } else {
        const first = s.synced === false;
        s.down = null; s.at = hm();
        if (A(s).role) {
          s.synced = true;
          const n = entriesOf(s), codes = n.filter((e) => e.totp).length;
          s.runs = [[`today ${hm()}`, `${(ms / 1000).toFixed(1)} s`, `${plural(n.length, "entry", "entries")}${first ? " listed" : ", none changed"}.${first && codes ? ` ${plural(codes, "code key")} kept in the keychain.` : ""}`], ...(s.runs || [])];
        }
        if (!quiet) toast(`Synced ${s.name}${A(s).role ? `: ${plural(entriesOf(s).length, "entry", "entries")}` : ""}.`);
        if (R.page === "passwords") apply(R);
      }
      render();
    }, ms);
    render();
  }

  /* ------------------------------------------------------------------ dialogs: the app's one dialog shell */
  const layer = $("#layer");
  function openDialog({ title, sub = "", body, footer = "", cls = "", actions = "", closeBtn = false }) {
    closeDialog();
    const prev = document.activeElement;
    const close = `<button type="button" class="tf-iconbtn tf-dclose" aria-label="Close" data-do="close"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8"/></svg></button>`;
    layer.innerHTML = `<div class="tf-modal"><div class="tf-dialog ${cls}" role="dialog" aria-modal="true" aria-labelledby="dlg-t" tabindex="-1">
      <header class="tf-dhead"><div class="tf-dtitle"><h2 id="dlg-t">${title}</h2>${sub ? `<p class="tf-dsub">${sub}</p>` : ""}</div>
        ${actions ? `<div class="tf-dctl"><div class="tf-acts">${actions}</div>${closeBtn ? `<span class="tf-dsep" aria-hidden="true"></span>${close}` : ""}</div>` : closeBtn ? close : ""}</header>
      <div class="tf-body" id="dlg-body">${body}</div>${footer ? `<footer id="dlg-foot">${footer}</footer>` : ""}</div></div>`;
    dlg = { prev, act: {} };
    const first = $("input:not([disabled]):not([type=radio]):not([type=checkbox]):not([type=range]), textarea", layer) || $(".tf-body button, .tf-body select", layer) || $("button", layer);
    first?.focus();
    return dlg;
  }
  function closeDialog() {
    if (!dlg) return;
    const prev = dlg.prev;
    dlg = null; layer.innerHTML = "";
    if (prev && document.contains(prev)) prev.focus();
  }

  /* ---- the generator ---- */
  const WORDS = ["anchor", "basalt", "cedar", "delta", "ember", "fjord", "gravel", "harbor", "inlet", "jetty", "kelp", "lagoon", "marsh", "nettle", "osprey", "pebble", "quay", "reef", "shoal", "tide", "upland", "vessel", "willow", "yarrow", "zephyr", "amber", "birch", "cobalt", "dune", "estuary", "flint", "gull", "heron", "iris", "juniper", "krill", "lichen", "mica", "nimbus", "otter", "plover", "quartz", "rudder", "sorrel", "tern", "umber", "violet", "wharf"];
  const rnd = (n) => crypto.getRandomValues(new Uint32Array(1))[0] % n;
  function pools(g) {
    const sets = [];
    if (g.upper) sets.push(g.plain ? "ABCDEFGHJKLMNPQRSTUVWXYZ" : "ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    if (g.lower) sets.push(g.plain ? "abcdefghijkmnopqrstuvwxyz" : "abcdefghijklmnopqrstuvwxyz");
    if (g.digits) sets.push(g.plain ? "23456789" : "0123456789");
    if (g.symbols) sets.push("!#$%&*+-=?@^_");
    return sets.length ? sets : ["abcdefghijklmnopqrstuvwxyz"];
  }
  function generate(g) {
    if (g.mode === "words") {
      const words = Array.from({ length: g.words }, () => { const w = WORDS[rnd(WORDS.length)]; return g.capital ? w[0].toUpperCase() + w.slice(1) : w; });
      if (g.number) words[rnd(words.length)] += String(rnd(90) + 10);
      return words.join(g.sep);
    }
    const sets = pools(g), all = sets.join("");
    const out = sets.map((set) => set[rnd(set.length)]);
    while (out.length < g.len) out.push(all[rnd(all.length)]);
    for (let i = out.length - 1; i > 0; i--) { const j = rnd(i + 1); [out[i], out[j]] = [out[j], out[i]]; }
    return out.slice(0, g.len).join("");
  }
  /* The words come from a short sample here; the line says what a real list of 7,776 gives. */
  const strength = (g) => (g.mode === "words"
    ? `${g.words} words from a list of 7,776${g.number ? " and a number" : ""}: about ${Math.round(g.words * 12.9 + (g.number ? 6.5 : 0))} bits.`
    : `${g.len} characters from ${pools(g).join("").length}: about ${Math.round(g.len * Math.log2(pools(g).join("").length))} bits.`);
  const newGen = () => ({ mode: "chars", len: 24, upper: true, lower: true, digits: true, symbols: true, plain: true, words: 5, sep: "-", capital: true, number: true });
  function genHtml(g) {
    const ck = (k, label) => `<label class="tf-ck"><input type="checkbox" data-g="${k}"${g[k] ? " checked" : ""}>${label}</label>`;
    return `<div class="pw-gen"><div class="pw-gen-row"><div class="tf-seg" role="group" aria-label="What to generate"><button type="button" aria-pressed="${g.mode === "chars"}" data-do="gmode" data-arg="chars">Characters</button><button type="button" aria-pressed="${g.mode === "words"}" data-do="gmode" data-arg="words">Words</button></div></div>
      ${g.mode === "chars"
        ? `<div class="pw-gen-row"><label for="g-len">Length</label><input type="range" id="g-len" min="8" max="64" value="${g.len}" data-g="len"><output id="g-len-out">${g.len}</output></div>
           <div class="pw-gen-row">${ck("upper", "A-Z")}${ck("lower", "a-z")}${ck("digits", "0-9")}${ck("symbols", "Symbols")}${ck("plain", "Leave out look-alikes (0 O 1 l I)")}</div>`
        : `<div class="pw-gen-row"><label for="g-words">Words</label><input type="range" id="g-words" min="3" max="8" value="${g.words}" data-g="words"><output id="g-words-out">${g.words}</output></div>
           <div class="pw-gen-row"><label for="g-sep">Between them</label><select id="g-sep" data-g="sep">${[["-", "Hyphen"], [".", "Full stop"], ["_", "Underscore"], [" ", "Space"]].map(([v, l]) => `<option value="${v}"${g.sep === v ? " selected" : ""}>${l}</option>`).join("")}</select>${ck("capital", "Capital first letters")}${ck("number", "Add a number")}</div>`}
      <p class="pw-strength" id="g-strength">${strength(g)}</p></div>`;
  }
  function genInput(g, el, valueInput) {
    const k = el.dataset.g;
    g[k] = el.type === "checkbox" ? el.checked : el.type === "range" ? Number(el.value) : el.value;
    if (el.type === "range") $(`#${el.id}-out`).textContent = el.value;
    valueInput.value = generate(g);
    $("#g-strength").textContent = strength(g);
  }

  /* ---- change a password ---- */
  function changeDialog(id) {
    const e = entry(id), s = src(e.src), g = newGen(), folder = commentFolder(e);
    const S = { value: generate(g), fail: "", busy: false };
    const where = e.url ? bare(e.url) : "the system it belongs to";
    const body = () => `<form id="chg-form" data-form="dlg" style="display: contents"><label class="tf-pane-h" for="g-val">New password</label>
      <div class="pw-newpw"><input id="g-val" type="text" value="${esc(S.value)}" spellcheck="false" autocomplete="off" required><button type="button" class="tf-btn" data-do="regen">Generate another</button></div>
      ${genHtml(g)}
      ${folder ? `<div class="tf-formgrid pw-form"><label for="chg-comment">Usage comment</label><input id="chg-comment" type="text" value="${esc(defaultComment())}" required></div><p class="tf-dlg-note">${esc(shown(s, folder).join(" / "))} requires one for every change too.</p>` : ""}
      <p class="tf-dlg-note">Save stores the new password in ${esc(sourceWord(s))}${cap(s, "history") ? " and keeps the old one in the entry's history" : ""}. It does not change it at ${esc(where)}: do that there, then save.</p>
      ${S.fail ? `<p class="tf-dlg-note tf-stg-errt" role="alert">${esc(S.fail)}</p>` : ""}</form>`;
    const foot = () => `${btn("Copy the new password", "copynew", { cls: "tf-first" })}${btn("Cancel", "close", { cls: "tf-ghost" })}<button class="tf-btn tf-primary" type="submit" form="chg-form"${S.busy ? " disabled" : ""}>${S.busy ? "Saving…" : "Save password"}</button>`;
    const d = openDialog({ title: `Change the password of ${esc(e.title)}`, body: body(), footer: foot() });
    const redraw = () => keepFocus(() => { $("#dlg-body").innerHTML = body(); $("#dlg-foot").innerHTML = foot(); });
    d.input = (el) => { if (el.dataset.g) genInput(g, el, $("#g-val")); S.value = $("#g-val").value; };
    d.act.regen = () => { S.value = generate(g); $("#g-val").value = S.value; };
    d.act.gmode = (mode) => { g.mode = mode; S.value = generate(g); redraw(); };
    d.act.copynew = () => { setClip(e, "new", "new password"); drawStatus(); tick(); toast("Copied the new password. The clipboard clears in 30 s."); };
    d.submit = () => {
      if (S.busy) return;
      const comment = $("#chg-comment")?.value.trim() || "";
      S.value = $("#g-val").value; S.busy = true; S.fail = ""; $("#dlg-foot").innerHTML = foot();
      setTimeout(() => {
        if (!dlg || dlg !== d) return;
        if (src(e.src).down) { S.busy = false; S.fail = `${s.name} does not answer, so nothing was saved. A new password is never kept waiting on this Mac.`; return redraw(); }
        if (cap(s, "history") === "earlier") e.earlier.unshift(TODAY);
        if (cap(s, "audit")) (V.log[e.id] ||= []).unshift([`today ${hm()}`, "You", "changed the password", comment]);
        if (comment) V.lastComment = comment;
        e.pw = S.value; e.changed = TODAY; e.by = ME.name; V.sec[`${e.id}|pw`] = {};
        closeDialog(); render();
        toast(`Saved the new password.${cap(s, "history") ? " The old one is in the entry's history." : ""}`);
      }, A(s).latency);
    };
  }

  /* ---- add or edit an entry: the folder decides where it goes and which fields there are ---- */
  function entryDialog(id) {
    const e = id ? entry(id) : null;
    const nodes = allNodes().filter((n) => { const s = srcOfNode(n); return s && s.synced && cap(s, "write") && (!e || s.id === e.src); });
    if (!nodes.length) return toast("No folder here takes a new entry.", true);
    const start = e ? nodes.find((n) => keyOf(n.segs) === keyOf(pathOf(e))) : nodes.find((n) => keyOf(n.segs) === keyOf(V.folder)) || nodes.find((n) => !n.top && !n.kids.length) || nodes[0];
    const g = newGen();
    const F = { folder: keyOf(start.segs), title: e?.title || "", user: e?.user || "", url: e?.url || "", notes: e?.notes || "", pw: "", show: false, totp: "", expires: e?.expires || "", comment: defaultComment(), busy: false, fail: "" };
    const nodeOf = () => nodes.find((n) => keyOf(n.segs) === F.folder);
    const body = () => {
      const n = nodeOf(), s = srcOfNode(n), needs = cap(s, "comments") && (s.commentFolders || []).find((f) => under(unshown(s, n.segs), f));
      return `<form class="tf-formgrid pw-form" id="entry-form" data-form="dlg">
        <label for="f-folder">Folder</label><select id="f-folder" data-f="folder" data-fk="f-folder">${nodes.map((x) => `<option value="${esc(keyOf(x.segs))}"${keyOf(x.segs) === F.folder ? " selected" : ""}>${esc(x.segs.join(" / "))}</option>`).join("")}</select>
        ${e ? `<p class="tf-dlg-note">An entry moves between folders of ${esc(multi() ? s.name : "its password manager")} only.</p>` : multi() ? `<p class="tf-dlg-note">It is saved in ${esc(s.name)}. The folder decides.</p>` : ""}
        <label for="f-title">Title</label><input id="f-title" type="text" data-f="title" value="${esc(F.title)}" autocomplete="off" required>
        <label for="f-user">Username</label><input id="f-user" type="text" class="tf-mono" data-f="user" value="${esc(F.user)}" autocomplete="off" spellcheck="false">
        ${e ? "" : `<label for="f-pw">Password</label><div class="pw-inl"><input id="f-pw" type="${F.show ? "text" : "password"}" class="tf-mono" data-f="pw" data-fk="f-pw" value="${esc(F.pw)}" autocomplete="new-password" spellcheck="false" required>${btn(F.show ? "Hide" : "Show", "fshow")}${btn("Generate", "fgen")}</div>`}
        <label for="f-url">Address</label><input id="f-url" type="text" class="tf-mono" data-f="url" value="${esc(F.url)}" autocomplete="off" spellcheck="false" placeholder="https://, ssh://, postgres://">
        ${cap(s, "totp") ? `<label for="f-totp">One-time code key</label><input id="f-totp" type="text" class="tf-mono" data-f="totp" value="${esc(F.totp)}" autocomplete="off" spellcheck="false" placeholder="${e?.totp ? "A key is kept. Paste a new one to replace it" : "A Base32 key or an otpauth:// link"}">` : ""}
        ${cap(s, "expiry") ? `<label for="f-expires">Expires</label><input id="f-expires" type="date" data-f="expires" value="${esc(F.expires)}">` : ""}
        <label for="f-notes">Notes</label><textarea id="f-notes" data-f="notes" rows="3">${esc(F.notes)}</textarea>
        ${needs ? `<label for="f-comment">Usage comment</label><input id="f-comment" type="text" data-f="comment" value="${esc(F.comment)}" required><p class="tf-dlg-note">${esc(shown(s, needs).join(" / "))} requires one for every change.</p>` : ""}
      </form>
      ${e ? `<p class="tf-dlg-note">Edit reads no password. To change the password, use Change password on the entry.</p>` : ""}
      ${F.fail ? `<p class="tf-dlg-note tf-stg-errt" role="alert">${esc(F.fail)}</p>` : ""}`;
    };
    const foot = () => `${btn("Cancel", "close", { cls: "tf-ghost" })}<button class="tf-btn tf-primary" type="submit" form="entry-form"${F.busy ? " disabled" : ""}>${F.busy ? "Saving…" : e ? "Save" : "Add entry"}</button>`;
    const d = openDialog({ title: e ? `Edit ${esc(e.title)}` : "New entry", body: body(), footer: foot() });
    const redraw = () => keepFocus(() => { $("#dlg-body").innerHTML = body(); $("#dlg-foot").innerHTML = foot(); });
    d.input = (el) => { if (!el.dataset.f) return; F[el.dataset.f] = el.value; if (el.dataset.f === "folder") redraw(); };
    d.act.fshow = () => { F.show = !F.show; redraw(); };
    d.act.fgen = () => { F.pw = generate(g); F.show = true; redraw(); };
    d.submit = () => {
      if (F.busy) return;
      const n = nodeOf(), s = srcOfNode(n);
      F.busy = true; F.fail = ""; $("#dlg-foot").innerHTML = foot();
      setTimeout(() => {
        if (!dlg || dlg !== d) return;
        if (s.down) { F.busy = false; F.fail = `${s.name} does not answer, so nothing was saved. ${e ? "Try again when it does." : "A new password is never kept waiting on this Mac."}`; return redraw(); }
        const path = unshown(s, n.segs), needs = cap(s, "comments") && (s.commentFolders || []).some((f) => under(path, f));
        const values = { title: F.title.trim(), user: F.user.trim(), url: F.url.trim(), notes: F.notes, expires: cap(s, "expiry") && F.expires ? F.expires : null, changed: TODAY, by: ME.name, path };
        let target = e;
        if (e) Object.assign(e, values, { totp: e.totp || (cap(s, "totp") && !!F.totp.trim()) });
        else { target = { id: `new-${++newId}`, src: s.id, fields: [], files: [], log: [], earlier: [], approver: null, readonly: false, ...values, pw: F.pw, totp: !!cap(s, "totp") && !!F.totp.trim() }; M.entries.push(target); }
        if (cap(s, "audit")) (V.log[target.id] ||= []).unshift([`today ${hm()}`, "You", e ? "changed the entry" : "added the entry", needs ? F.comment.trim() : ""]);
        if (needs && F.comment.trim()) V.lastComment = F.comment.trim();
        closeDialog();
        V.q = "";
        go(`#/passwords/entry/${target.id}`, true);
        toast(e ? `Saved ${target.title}.` : `Added ${target.title} to ${n.segs.join(" / ")}.`);
      }, A(s).latency);
    };
  }

  /* ---- an attachment, in the app's file viewer ---- */
  function fileDialog(e, f) {
    const text = FILES[f.name];
    const stage = text ? `<div class="tf-vstage" data-shows="text"><pre class="pw-pre">${esc(text)}</pre></div><div class="tf-vnote">Made up for this mockup. Read just now and held in memory; closing drops it.</div>`
      : `<div class="tf-vstage"><div class="tf-vempty"><span class="tf-tkkind" aria-hidden="true">${esc(f.kind)}</span><p>No preview for a ${esc(f.kind)} file. Download saves it to your Downloads folder.</p></div></div>`;
    const d = openDialog({ title: esc(f.name), sub: `${esc(f.size)}, attached to ${esc(e.title)}`, cls: "tf-viewer", closeBtn: true, actions: btn("Download", "dl", { cls: "" }), body: stage });
    d.act.dl = () => toast(`Saved ${f.name} to Downloads.`);
  }

  /* ---- Settings › Sources: Add a source, in the app's own dialog ---- */
  function seedFor(kind) {
    const blank = { id: `${kind}-2`, name: ADAPTERS[kind].name, address: "", user: "", folders: [] };
    if (!ADAPTERS[kind].role) return blank;
    const first = DATA.SOURCES.find((s) => s.adapter === kind);
    if (!src(first.id)) return first;
    return src(DATA.EXTRA[kind].id) ? blank : DATA.EXTRA[kind];
  }
  function addSourceDialog(prefer) {
    const kinds = Object.keys(ADAPTERS);
    const D = { kind: prefer || kinds[0], vals: {}, report: null, testing: false, manifest: "" };
    const fill = () => {
      const seed = seedFor(D.kind), a = ADAPTERS[D.kind];
      D.vals = { address: seed.address || "", user: seed.user || "", client_id: D.kind === "bitwarden" ? "user.4f1c9a2e-77b0-4c1d" : "", client_secret: D.kind === "bitwarden" ? "mockup-client-secret" : "", secret: a.role ? "mockup-master-password" : "", name: seed.name, id: seed.id };
      D.report = null;
    };
    fill();
    const ready = () => (D.vals.address.trim() !== "" || D.kind === "bitwarden") && D.vals.secret !== "" && /^[a-z][a-z0-9-]*$/.test(D.vals.id);
    const field = (k, label, o = {}) => `<label for="add-${k}">${label}</label><input id="add-${k}" type="${o.secret ? "password" : "text"}" data-d="${k}" value="${esc(D.vals[k] ?? "")}" autocomplete="off" spellcheck="false"${o.ph ? ` placeholder="${esc(o.ph)}"` : ""}>`;
    const body = () => {
      const a = ADAPTERS[D.kind];
      const idError = !/^[a-z][a-z0-9-]*$/.test(D.vals.id) ? "Lower-case letters, digits and hyphens, starting with a letter." : src(D.vals.id) ? "A source already has this id." : "";
      return `<div class="tf-pane-h">What is it?</div>
        <div class="tf-radio" role="radiogroup" aria-label="What is it?">${kinds.map((k) => `<label><input type="radio" name="add-adapter" value="${k}" data-d="kind"${k === D.kind ? " checked" : ""}><span>${esc(ADAPTERS[k].name)}<small>${esc(ADAPTERS[k].holds)}</small></span></label>`).join("")}</div>
        <div class="tf-formgrid"><label for="add-manifest">Install an adapter</label><div class="tf-add-inst"><input id="add-manifest" type="text" autocomplete="off" placeholder="A repository URL, a manifest URL or a file"><button type="button" class="tf-btn" disabled>Install…</button></div></div>
        <p class="tf-dlg-note">A GitHub repository, a manifest URL or a local manifest. Install checks it and shows what it may reach before anything is installed.</p>
        <div class="tf-formgrid">${field("address", "Address", { ph: a.placeholder || "https://" })}
          ${(a.fields || (a.user ? [{ k: "user", l: "User" }] : [])).map((f) => field(f.k, f.l, { secret: f.secret, ph: f.ph })).join("")}
          ${field("secret", a.secret, { secret: true })}${field("name", "Name")}${field("id", "Id")}</div>
        <p class="tf-dlg-note${idError ? " tf-stg-errt" : ""}">${idError || "Permanent: everything from this source is named with it."}</p>
        ${a.role ? `<p class="tf-dlg-note">${esc(a.kept)} Nothing locks, and you are not asked for it again.</p>` : ""}
        ${a.role && a.everyDefault === 0 ? `<p class="tf-dlg-note">It syncs only when you press Sync now, so nothing is read from it in the background. Its page changes that.</p>` : ""}
        ${D.report ? `<p class="tf-dlg-note${D.report.ok ? "" : " tf-stg-errt"}" role="status">${esc(D.report.text)}</p>` : `<p class="tf-dlg-note">Test connection signs in and says what it found.${a.role ? " It reads no entries and no passwords." : ""}</p>`}`;
    };
    const foot = () => `<button type="button" class="tf-btn tf-first" data-do="addtest"${!ready() || D.testing ? " disabled" : ""}>${D.testing ? "Testing…" : "Test connection"}</button>${btn("Cancel", "close", { cls: "tf-ghost" })}<button type="button" class="tf-btn tf-primary" data-do="addsave"${D.report?.ok && !src(D.vals.id) ? "" : " disabled"}>Add source</button>`;
    const d = openDialog({ title: "Add a source", body: body(), footer: foot() });
    const redraw = () => keepFocus(() => { $("#dlg-body").innerHTML = body(); $("#dlg-foot").innerHTML = foot(); });
    $(`input[name="add-adapter"][value="${D.kind}"]`, layer)?.closest("label").scrollIntoView({ block: "nearest" });
    d.input = (el) => {
      if (el.dataset.d === "kind") { D.kind = el.value; fill(); return redraw(); }
      if (!el.dataset.d) return;
      D.vals[el.dataset.d] = el.value; D.report = null; $("#dlg-foot").innerHTML = foot();
    };
    d.act.addtest = () => {
      D.testing = true; $("#dlg-foot").innerHTML = foot();
      setTimeout(() => {
        if (dlg !== d) return;
        const who = D.vals.user || "mara";
        D.testing = false;
        D.report = { ok: true, text: D.kind === "pleasant" ? `Signed in as ${who}. Password Server 8.2, API v5. 26 folders you can read.`
          : D.kind === "bitwarden" ? `Signed in as ${who} with the API key. The master password opened the vault: your own folders and 1 organization.`
          : `Signed in as ${who}.` };
        redraw();
      }, 900);
    };
    d.act.addsave = () => {
      const a = ADAPTERS[D.kind], seed = seedFor(D.kind);
      const s = { ...clone(seed), id: D.vals.id, adapter: D.kind, name: D.vals.name.trim() || a.name, address: D.vals.address.trim(), user: D.vals.user || seed.user || "", every: a.role ? a.everyDefault : 900, at: a.role ? null : hm(), runs: [] };
      if (a.role) { s.synced = false; s.commentFolders = []; delete s.down; for (const e of clone(DATA.ENTRIES).filter((x) => x.src === seed.id)) M.entries.push({ ...e, src: s.id }); }
      else s.holds = "0 items";
      M.sources.push(s);
      closeDialog();
      go(`#/settings/source/${s.id}`, true);
      if (a.role && s.every === 0) toast(`Added ${s.name}. It syncs only when you press Sync now.`);
      else { toast(`Added ${s.name}. Syncing now.`); syncSource(s, true); }
    };
  }

  function credDialog(s) {
    const a = A(s), word = a.secret.toLowerCase(), role = !!a.role;
    const d = openDialog({
      title: `${role ? "Change" : "Re-enter"} the ${esc(s.name)} ${word}`,
      body: `<form class="tf-formgrid" id="cred-form" data-form="dlg"><label for="cred-secret">${a.secret}</label><input id="cred-secret" type="password" autocomplete="off" placeholder="${a.secret}" required>
        ${s.adapter === "bitwarden" ? `<label for="cred-key">API key client_secret</label><input id="cred-key" type="password" autocomplete="off" placeholder="Leave empty to keep the stored one">` : ""}</form>
        <p class="tf-dlg-note">${role ? `This tells knobas the new ${word}. It does not change it at ${esc(bare(s.address) || "bitwarden.com")}. ` : ""}Stored in macOS Keychain.${role ? " Nothing locks, and you are not asked for it again." : ` Writes waiting for ${esc(s.name)} go out once it signs in.`}</p>`,
      footer: `${btn("Cancel", "close", { cls: "tf-ghost" })}<button class="tf-btn tf-primary" type="submit" form="cred-form">Sign in</button>`,
    });
    d.submit = () => {
      $("#dlg-foot button[type=submit]").disabled = true;
      setTimeout(() => {
        if (dlg !== d) return;
        closeDialog();
        if (s.every === 0) toast(`Signed in to ${s.name}. It was not synced: it syncs only when you press Sync now.`);
        else { toast(`Signed in to ${s.name}. Syncing now.`); syncSource(s, true); }
      }, 700);
    };
  }

  function removeDialog(s) {
    const d = openDialog({
      title: `Remove ${esc(s.name)}?`,
      body: `<div class="tf-radio" role="radiogroup" aria-label="What it synced"><label><input type="radio" name="rm" value="keep" checked><span>Keep what it synced<small>Items stay searchable and linked, and stop updating.</small></span></label>
        <label><input type="radio" name="rm" value="purge"><span>Delete what it synced<small>Links to those items go too. Your links between other items stay.</small></span></label></div>
        ${A(s).role ? `<p class="tf-dlg-note">Either way its credential and its one-time code keys are deleted from macOS Keychain, so kept entries can no longer show a password or a code.</p>` : ""}`,
      footer: `${btn("Cancel", "close", { cls: "tf-ghost" })}${btn("Remove", "rmgo", { cls: "tf-primary tf-danger" })}`,
    });
    d.act.rmgo = () => {
      M.sources = M.sources.filter((x) => x !== s); M.entries = M.entries.filter((e) => e.src !== s.id);
      V.recent = V.recent.filter((id) => entry(id)); delete V.sync[s.id];
      closeDialog(); go("#/settings/sources", true); toast(`Removed ${s.name}`);
    };
  }

  /* ------------------------------------------------------------------ what a press does */
  const cur = () => (V.entry ? entry(V.entry) : null);
  function select(id, { pane = V.pane, focus = false } = {}) {
    V.entry = id; V.pane = pane;
    history.replaceState(null, "", `${location.search}#/passwords/entry/${id}`);
    R = parse();
    $("#pw-cols")?.setAttribute("data-pane", V.pane);
    for (const row of $$(".pw-row")) row.dataset.arg === id ? row.setAttribute("aria-current", "true") : row.removeAttribute("aria-current");
    part("#pw-tree", treeHtml);
    drawEntry();
    if (focus) { const row = $$(".pw-row").find((r) => r.dataset.arg === id); row?.focus({ preventScroll: true }); row?.scrollIntoView({ block: "nearest" }); }
  }
  const DO = {
    open: (id) => select(id, { pane: "entry" }),
    back: () => { V.pane = "list"; $("#pw-cols")?.setAttribute("data-pane", "list"); },
    drawer: () => { V.drawer = !V.drawer; render(); },
    clearq: () => { V.q = ""; render(); $("#pw-q")?.focus(); },
    folder: (k, el, ev) => {
      const segs = segsOf(k), n = nodeAt(segs);
      const toggle = () => { V.open.has(k) ? V.open.delete(k) : V.open.add(k); drawTree(); };
      if (ev?.target.closest?.("[data-twist]") && n?.kids.length) return toggle();
      if (k === keyOf(V.folder) && !V.q && k !== "" && n?.kids.length) return toggle();
      V.open.add(k); V.drawer = false; V.q = "";
      go(k ? `#/passwords/in/${k}` : "#/passwords", true);
    },
    show: (k) => { const e = entry(k.split("|")[0]); read(e, k, "show", { words: `showed the ${secretName(k)}`, run: (st) => { st.shownUntil = now() + HOLD * 1000; } }); },
    hide: (k) => { if (V.sec[k]) V.sec[k].shownUntil = 0; drawEntry(); },
    copy: (k) => {
      const e = entry(k.split("|")[0]), st = V.sec[k] || {};
      /* A password already on screen is copied from there: no second read, no second line in the log. */
      if ((st.shownUntil || 0) > now()) { setClip(e, k, secretName(k)); drawEntry(); drawStatus(); tick(); return; }
      read(e, k, "copy", { words: `copied the ${secretName(k)}`, run: () => setClip(e, k, secretName(k)) });
    },
    copyotp: (id) => { const e = entry(id); if (!e || isBlocked(e)) return; setClip(e, `${id}|otp`, "one-time code"); touch(e); drawEntry(); drawStatus(); },
    copyplain: (what) => { V.clip = null; drawEntry(); drawStatus(); toast(`Copied the ${what.toLowerCase()}.`); },
    address: (id) => { const e = entry(id); if (isWeb(e.url)) toast(`Opened ${e.url} in your browser.`); else DO.copyplain("address"); },
    clearclip: () => { V.clip = null; drawEntry(); drawStatus(); toast("Clipboard cleared."); },
    askcancel: (k) => { const st = V.sec[k]; if (st) { st.ask = null; st.pending = null; } drawEntry(); },
    askuse: () => { const i = $("#ask-in"); if (i) { i.value = jobComment(); i.focus(); i.select(); } },
    reqopen: (id) => { V.req[id] = { st: "form" }; drawEntry(); const i = $("#req-in"); i?.focus(); i?.select(); },
    reqcancel: (id) => { delete V.req[id]; drawEntry(); },
    reqcheck: (id) => {
      const r = V.req[id], e = entry(id);
      r.checking = true; drawEntry();
      setTimeout(() => {
        if (V.req[id] !== r) return;
        r.checking = false;
        if (V.approved[id]) { r.st = "approved"; r.until = hm(now() + 2 * 3600e3); toast(`${e.approver} approved your request.`); drawMk(); }
        else r.checked = hm();
        drawEntry();
      }, A(src(e.src)).latency);
    },
    fileopen: (k) => { const e = entry(k.split("|")[0]), f = e.files[+k.split("|")[1].slice(4)]; read(e, k, "open", { words: `opened ${f.name}`, run: () => fileDialog(e, f) }); },
    filesave: (k) => { const e = entry(k.split("|")[0]), f = e.files[+k.split("|")[1].slice(4)]; read(e, k, "save", { words: `saved ${f.name}`, run: (st) => { st.note = `Saved to Downloads at ${hm()}.`; toast(`Saved ${f.name} to Downloads.`); } }); },
    hist: (id) => {
      const e = entry(id), h = V.hist[id] || "closed";
      if (h !== "closed") { V.hist[id] = "closed"; return drawEntry(); }
      V.hist[id] = "reading"; drawEntry();
      setTimeout(() => { if (V.hist[id] === "reading") { V.hist[id] = "open"; drawEntry(); } }, A(src(e.src)).latency);
    },
    retry: (id) => {
      const s = src(id);
      V.test[id] = "busy"; render();
      setTimeout(() => {
        delete V.test[id];
        if (s.unreachable) { s.down = hm(); toast(`${s.name} still does not answer.`, true); } else { s.down = null; toast(`${s.name} answers again.`); }
        render();
      }, 1400);
    },
    change: (id) => changeDialog(id),
    edit: (id) => entryDialog(id),
    new: () => entryDialog(null),
    syncrole: () => pwSources().forEach((s) => syncSource(s)),
    launcher: () => toast("The launcher finds entries too, since they are in the mirror. It is not drawn in this mockup."),
    timer: () => toast(`The timer runs on ${JOB.key}. A usage comment starts from that ticket.`),
    theme: () => setTheme(document.documentElement.dataset.theme === "dark" || (!theme && matchMedia("(prefers-color-scheme: dark)").matches) ? "light" : "dark"),
    close: () => closeDialog(),
    /* Settings */
    addpw: () => { go("#/settings/sources", true); addSourceDialog(["pleasant", "bitwarden"].find((k) => !pwSources().some((s) => s.adapter === k)) || "pleasant"); },
    addsource: () => addSourceDialog(),
    syncall: () => { M.sources.filter((s) => !s.paused).forEach((s) => syncSource(s, true)); toast(`Syncing all ${M.sources.length} sources`); },
    syncsource: (id) => syncSource(src(id)),
    cred: (id) => credDialog(src(id)),
    remove: (id) => removeDialog(src(id)),
    pause: (id) => { const s = src(id); s.paused = !s.paused; render(); toast(s.paused ? "Paused. Nothing syncs until you turn it on; writes you make still go." : `${s.name} syncs again.`); },
    test: (id) => {
      const s = src(id);
      V.test[id] = "busy"; render();
      setTimeout(() => {
        V.test[id] = s.unreachable ? { ok: false, text: `No answer from ${bare(s.address)}.` }
          : { ok: true, text: s.adapter === "pleasant" ? `Signed in as ${s.user}. Password Server 8.2, API v5. It read no entries.` : s.adapter === "bitwarden" ? `Signed in as ${s.user} with the API key. The master password opened the vault.` : `Signed in${s.user ? ` as ${s.user}` : ""}.` };
        if (s.unreachable) s.down = hm();
        render();
      }, 900);
    },
  };
  const FORMS = {
    ask: (k, form) => {
      const st = V.sec[k], comment = $("input", form).value.trim();
      if (!st || !comment) return;
      st.comment = comment; V.lastComment = comment;
      read(entry(k.split("|")[0]), k, st.ask, st.pending);
    },
    req: (id, form) => {
      const comment = $("input", form).value.trim();
      if (!comment) return;
      V.req[id] = { st: "pending", at: hm(), comment }; V.lastComment = comment;
      drawEntry(); drawMk();
      toast(`Request sent to ${entry(id).approver}.`);
    },
  };

  /* ------------------------------------------------------------------ the address */
  function parse() {
    const parts = location.hash.replace(/^#\/?/, "").split("/");
    if (parts[0] === "settings") {
      if (parts[1] === "source") return { page: "settings", section: "sources", source: decodeURIComponent(parts[2] || "") };
      return { page: "settings", section: SECTIONS.some(([k]) => k === parts[1]) ? parts[1] : "sources" };
    }
    if (parts[0] === "passwords" || parts[0] === "") {
      if (parts[1] === "entry") return { page: "passwords", entry: decodeURIComponent(parts[2] || "") };
      return { page: "passwords", folder: parts[1] === "in" ? parts.slice(2).map(decodeURIComponent) : [] };
    }
    const names = { tickets: "Tickets", inbox: "Inbox", code: "Code", builds: "Builds", docs: "Docs", deps: "Dependencies", monitors: "Monitors", sets: "Link sets", review: "Proposed links", time: "Time" };
    return { page: "other", name: names[parts[0]] || "This page" };
  }
  function apply(r) {
    if (r.page !== "passwords") return;
    TREE = buildTree();
    const e = r.entry ? entry(r.entry) : null;
    if (e) {
      const p = pathOf(e);
      if (!nodeAt(V.folder) || !under(p, V.folder)) V.folder = p;
      V.entry = e.id; V.pane = "entry";
    } else {
      V.folder = r.folder && nodeAt(r.folder) ? r.folder : [];
      V.pane = "list";
      V.entry = firstListed()?.id || null;
    }
    V.folder.forEach((_, i) => V.open.add(keyOf(V.folder.slice(0, i + 1))));
    /* The sources' top folders start open, so the first level is in view. */
    if (!V.opened) { V.opened = true; TREE.kids.forEach((k) => k.top && V.open.add(keyOf(k.segs))); }
  }
  function go(hash, force = false) {
    if (location.hash === hash) { if (force) { R = parse(); apply(R); render(); } return; }
    location.hash = hash;
  }
  addEventListener("hashchange", () => { R = parse(); V.drawer = false; apply(R); render(); });

  /* ------------------------------------------------------------------ events */
  document.addEventListener("click", (ev) => {
    const mkb = ev.target.closest?.("[data-mk]");
    if (mkb) return mkAct(mkb.dataset.mk, mkb.dataset.arg);
    if (ev.target.classList?.contains("tf-modal")) return closeDialog();
    const el = ev.target.closest?.("[data-do]");
    if (!el || el.disabled) return;
    const fn = (dlg && dlg.act[el.dataset.do]) || DO[el.dataset.do];
    if (!fn) return;
    if (el.tagName === "A") ev.preventDefault();
    fn(el.dataset.arg ?? "", el, ev);
  });
  document.addEventListener("input", (ev) => {
    const t = ev.target;
    if (t.id === "pw-q") {
      V.q = t.value;
      TREE = buildTree();
      const rows = listing()[0]?.rows || [];
      if (V.q) V.entry = rows[0]?.id || null; else if (!cur()) V.entry = firstListed()?.id || null;
      return drawList();
    }
    if (t.dataset.draft) { const st = V.sec[t.dataset.draft]; if (st) st.draft = t.value; return; }
    if (dlg && layer.contains(t)) dlg.input?.(t);
  });
  document.addEventListener("change", (ev) => {
    const t = ev.target;
    if (t.dataset.change !== "every") return;
    const s = src(t.dataset.arg);
    s.every = Number(t.value);
    render();
    toast(s.every === 0 ? `${s.name} syncs only when you press Sync now.` : `${s.name} syncs ${everyWords(s.every).toLowerCase()}`);
  });
  document.addEventListener("submit", (ev) => {
    ev.preventDefault();
    const f = ev.target;
    if (dlg && layer.contains(f)) return dlg.submit?.(f);
    FORMS[f.dataset.form]?.(f.dataset.arg, f);
  });

  function moveRow(dir) {
    const rows = $$(".pw-row");
    if (!rows.length) return;
    const i = rows.findIndex((r) => r.dataset.arg === V.entry);
    const next = rows[Math.max(0, Math.min(rows.length - 1, i < 0 ? 0 : i + dir))];
    select(next.dataset.arg, { focus: true });
  }
  document.addEventListener("keydown", (ev) => {
    const t = ev.target;
    if (dlg) {
      if (ev.key === "Escape") { ev.preventDefault(); return closeDialog(); }
      if (ev.key !== "Tab") return;
      const items = $$('a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled])', layer);
      if (!items.length) return;
      if (ev.shiftKey && document.activeElement === items[0]) { ev.preventDefault(); items[items.length - 1].focus(); }
      else if (!ev.shiftKey && document.activeElement === items[items.length - 1]) { ev.preventDefault(); items[0].focus(); }
      return;
    }
    if (R.page !== "passwords") {
      if (ev.key === "Escape" && R.page === "settings" && R.source && !t.matches("input, select, textarea")) go("#/settings/sources");
      return;
    }
    if (t.id === "pw-q") {
      if (ev.key === "Escape") { ev.preventDefault(); if (V.q) DO.clearq(); else t.blur(); }
      if (ev.key === "ArrowDown" || ev.key === "Enter") { ev.preventDefault(); const row = $$(".pw-row").find((r) => r.dataset.arg === V.entry) || $(".pw-row"); if (row) select(row.dataset.arg, { pane: ev.key === "Enter" ? "entry" : V.pane, focus: true }); }
      return;
    }
    if (t.matches("input, textarea, select")) {
      if (ev.key === "Escape") { ev.preventDefault(); t.closest("form")?.querySelector('[data-do$="cancel"]')?.click(); }
      return;
    }
    if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
    if (t.classList.contains("pw-node")) {
      const nodes = $$(".pw-node"), i = nodes.indexOf(t), k = t.dataset.arg, level = Number(t.getAttribute("aria-level"));
      if (ev.key === "ArrowDown") { ev.preventDefault(); return nodes[i + 1]?.focus(); }
      if (ev.key === "ArrowUp") { ev.preventDefault(); return nodes[i - 1]?.focus(); }
      if (ev.key === "ArrowRight") { ev.preventDefault(); if (t.getAttribute("aria-expanded") === "false") { V.open.add(k); drawTree(); } else if (t.getAttribute("aria-expanded") === "true") nodes[i + 1]?.focus(); return; }
      if (ev.key === "ArrowLeft") { ev.preventDefault(); if (t.getAttribute("aria-expanded") === "true") { V.open.delete(k); drawTree(); } else nodes.slice(0, i).reverse().find((n) => Number(n.getAttribute("aria-level")) < level)?.focus(); return; }
    }
    const key = ev.key.length === 1 ? ev.key.toLowerCase() : ev.key;
    const e = cur();
    const press = (name) => { const b = $$(`#pw-entrycol [data-do="${name}"]`)[0]; if (b && !b.disabled) { ev.preventDefault(); b.click(); } };
    if (key === "/") { ev.preventDefault(); $("#pw-q")?.focus(); $("#pw-q")?.select(); }
    else if (key === "ArrowDown" || key === "j") { ev.preventDefault(); moveRow(1); }
    else if (key === "ArrowUp" || key === "k") { ev.preventDefault(); moveRow(-1); }
    else if (key === "n") { ev.preventDefault(); $('[data-do="new"]') && DO.new(); }
    else if (key === "Escape") {
      if (V.q) DO.clearq();
      else if (V.pane === "entry" && matchMedia("(max-width: 700px)").matches) DO.back();
      else if (V.folder.length) go(V.folder.length > 1 ? `#/passwords/in/${keyOf(V.folder.slice(0, -1))}` : "#/passwords", true);
    }
    else if (!e) return;
    else if (key === "c") { ev.preventDefault(); if (!src(e.src).down && !isBlocked(e)) DO.copy(`${e.id}|pw`); }
    else if (key === "s") { ev.preventDefault(); const k = `${e.id}|pw`; (V.sec[k]?.shownUntil || 0) > now() ? DO.hide(k) : DO.show(k); }
    else if (key === "u") press("copyplain");
    else if (key === "o") press("copyotp");
    else if (key === "a") press("address");
    else if (key === "e") press("edit");
  });

  /* ------------------------------------------------------------------ the mockup pill: scenario and theme */
  const mk = $("#mk");
  function drawMk() {
    if (!mk) return;
    const label = SCENARIOS.find(([k]) => k === scenario)[1];
    const waiting = Object.keys(V.req).filter((id) => V.req[id].st === "pending");
    const approve = waiting.map((id) => `<button type="button" data-mk="approve" data-arg="${id}" aria-pressed="${!!V.approved[id]}">${V.approved[id] ? "Approved at the server. Press Check now on the entry" : `${esc(entry(id)?.approver || "The approver")} approves the request`}</button>`).join("");
    const menu = mkOpen ? `<div class="mk-menu"><small>Scenario</small>${SCENARIOS.map(([k, l]) => `<button type="button" data-mk="scenario" data-arg="${k}" aria-pressed="${k === scenario}">${l}</button>`).join("")}
      <small>Theme</small><div class="mk-row">${[["light", "Light"], ["dark", "Dark"]].map(([t, l]) => `<button type="button" data-mk="theme" data-arg="${t}" aria-pressed="${theme === t}">${l}</button>`).join("")}</div></div>` : "";
    mk.innerHTML = `${menu}${approve}<button type="button" class="mk-tog" data-mk="toggle" aria-expanded="${mkOpen}"><b>Mockup</b>${esc(label)} ${mkOpen ? "▾" : "▴"}</button>`;
  }
  function setUrl() {
    const p = new URLSearchParams(location.search);
    p.set("scenario", scenario);
    theme ? p.set("theme", theme) : p.delete("theme");
    history.replaceState(null, "", `?${p}${location.hash}`);
  }
  function setTheme(t) {
    theme = theme === t ? "" : t;
    theme ? (document.documentElement.dataset.theme = theme) : delete document.documentElement.dataset.theme;
    setUrl(); drawMk();
  }
  function mkAct(what, arg) {
    if (what === "toggle") { mkOpen = !mkOpen; return drawMk(); }
    if (what === "theme") return setTheme(arg);
    if (what === "approve") { V.approved[arg] = true; return drawMk(); }
    if (what === "scenario") {
      scenario = arg; mkOpen = false;
      closeDialog(); load();
      history.replaceState(null, "", `${location.search}${R.page === "settings" ? "#/settings/sources" : "#/passwords"}`);
      setUrl();
      R = parse(); apply(R); render();
    }
  }

  /* ------------------------------------------------------------------ start */
  if (theme) document.documentElement.dataset.theme = theme;
  if (params.get("bar") === "0") document.body.classList.add("mk-hide");
  load();
  R = parse(); apply(R); render();
  setInterval(tick, 250);
})();
