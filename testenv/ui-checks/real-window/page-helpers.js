// Helpers real-window.sh installs in the page as window.__q: where to press, what has focus, how far a box scrolled.
window.__q = {
  box(sel, i = 0) { const e = [...document.querySelectorAll(sel)].filter(e => e.getClientRects().length)[i]; if (!e) return null; const r = e.getBoundingClientRect(); return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)]; },
  rows() { return [...document.querySelectorAll(".tf-ib [data-act=done]")].length; },
  focus() { return window.__rwName(document.activeElement); },
  toast() { return [...document.querySelectorAll("[role=status], .tf-toast, [class*=toast]")].map(e => e.innerText.trim()).filter(Boolean).join(" | "); },
  log(n = 12) { return window.__rwLog.slice(-n); },
};
window.__q.btn = (text, i = 0) => { const b = [...document.querySelectorAll("button, a")].filter(b => b.getClientRects().length && (b.innerText || "").trim() === text)[i]; if (!b) return null; const r = b.getBoundingClientRect(); return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)]; };
window.__q.textAt = (sel) => { const box = [...document.querySelectorAll(sel)].find(e => e.getClientRects().length && !e.closest("[hidden]")); if (!box) return null; const w = document.createTreeWalker(box, NodeFilter.SHOW_TEXT); for (let n = w.nextNode(); n; n = w.nextNode()) { if (n.textContent.trim().length < 12 || n.parentElement.closest("a,button,input,select,textarea,summary,label")) continue; const r = document.createRange(); const a = n.textContent.search(/\S/); r.setStart(n, a); r.setEnd(n, a + 8); const q = r.getClientRects()[0]; if (!q || q.width < 4 || q.top < box.getBoundingClientRect().top + 4 || q.bottom > Math.min(box.getBoundingClientRect().bottom, innerHeight) - 30) continue; const u = document.elementFromPoint(q.left + q.width / 2, q.top + q.height / 2); if (!u || !box.contains(u) || u.closest("a,button,input,select,textarea,summary,label")) continue; return [Math.round(q.left + q.width / 2), Math.round(q.top + q.height / 2)]; } return null; };
window.__q.top = (sel) => { const e = [...document.querySelectorAll(sel)].find(e => e.getClientRects().length && !e.closest("[hidden]")); return e ? Math.round(e.scrollTop) : -1; };
window.__q.zero = (sel) => { [...document.querySelectorAll(sel)].forEach(e => e.scrollTop = 0); return 0; };
window.__q.starts = (text, sel = "button, a") => { const b = [...document.querySelectorAll(sel)].filter(b => b.getClientRects().length && (b.innerText || "").trim().startsWith(text))[0]; if (!b) return null; const r = b.getBoundingClientRect(); return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)]; };
"ok"
