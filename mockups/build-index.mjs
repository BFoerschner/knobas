// Builds mockups/round-N/index.html from the header comments of the mockups.
// Usage: node mockups/build-index.mjs round-1
import { readdirSync, readFileSync, writeFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const round = process.argv[2] ?? 'round-1';
const root = new URL('.', import.meta.url).pathname;
const roundDir = join(root, round);
const sharedDir = join(root, 'shared');

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

function briefSummary(dir) {
  const out = {};
  for (const f of readdirSync(dir).filter((f) => f.endsWith('.md')).sort()) {
    const text = readFileSync(join(dir, f), 'utf8');
    const id = f.split('-')[0];
    const name = (text.match(/^# \w+ — (.+)$/m) ?? [])[1] ?? f;
    const thesis = (text.match(/\*\*(?:Thesis|Identity):\*\* (.+)$/m) ?? [])[1] ?? '';
    out[id] = { id, name, thesis, slug: f.replace(/\.md$/, '') };
  }
  return out;
}

const paradigms = briefSummary(join(sharedDir, 'paradigms'));
const designs = briefSummary(join(sharedDir, 'designs'));

function parseHeader(html) {
  const m = html.match(/<!--\s*knobas mockup([\s\S]*?)-->/);
  const h = { thesis: [] };
  if (!m) return h;
  for (const line of m[1].split('\n')) {
    const mm = line.match(/^\s*(\w+):\s*(.+?)\s*$/);
    if (!mm) continue;
    if (mm[1] === 'thesis') h.thesis.push(mm[2]);
    else h[mm[1]] = mm[2];
  }
  return h;
}

// ---- list mode: round dirs that hold W*.html (or any flat *.html) instead of P*/ dirs
const flat = readdirSync(roundDir).filter((f) => /^[A-Z]\d.*\.html$/.test(f) || /^base.*\.html$/.test(f)).sort();
if (flat.length) {
  const items = flat.map((f) => {
    const path = join(roundDir, f);
    const html = readFileSync(path, 'utf8');
    return { href: f, lines: html.split('\n').length, kb: Math.round(statSync(path).size / 1024), header: parseHeader(html) };
  });
  const li = items.map((c) => `<li class="item">
    <a class="open" href="${c.href}" target="_blank" rel="noopener"><span class="id">${esc(c.header.way ?? c.header.paradigm ?? c.href)}</span><span class="title">${esc(c.header.design ?? '')}</span></a>
    <ul class="thesis">${c.header.thesis.map((t) => `<li>${esc(t)}</li>`).join('')}</ul>
    <span class="meta">${c.lines} lines · ${c.kb} KB</span></li>`).join('');
  const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>knobas mockups — ${esc(round)}</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  :root { color-scheme: light dark; --line: #8884; --muted: #888; --accent: #3b6fd6; }
  body { margin: 0; font: 14px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif; padding: 32px 40px; max-width: 900px; }
  h1 { font-size: 22px; margin: 0 0 4px; } h1 small { font-weight: 400; color: var(--muted); margin-left: 8px; }
  .intro { color: var(--muted); margin: 0 0 20px; } .intro a { color: var(--accent); }
  ul.list { list-style: none; padding: 0; margin: 0; } .item { border-top: 1px solid var(--line); padding: 14px 0; }
  .open { text-decoration: none; color: inherit; display: block; } .open:hover .id { color: var(--accent); text-decoration: underline; }
  .id { font-weight: 700; font-size: 16px; margin-right: 10px; } .title { color: var(--muted); }
  .thesis { margin: 6px 0 8px; padding-left: 16px; font-size: 13px; color: var(--muted); }
  .meta { font-size: 11px; color: var(--muted); font-family: ui-monospace, Menlo, monospace; }
</style></head><body>
<h1>knobas mockups <small>${esc(round)} · ${items.length} built</small></h1>
<p class="intro">All on the Signal shell (context hub + launcher box). Same example data: <a href="../shared/dataset.md">dataset.md</a> + <a href="../shared/assets.md">assets.md</a>; required surfaces in <a href="../shared/round-2-brief.md">round-2-brief.md</a>. Try: <kbd>⌘K</kbd> then <code>asset:</code>, the PAY-231 room's ASSETS tile, and promoting <code>sepa-payout-export</code> stage → prod.</p>
<ul class="list">${li}</ul>
</body></html>
`;
  writeFileSync(join(roundDir, 'index.html'), html);
  console.log(`${round}: ${items.length} items → ${join(roundDir, 'index.html')}`);
  process.exit(0);
}

const cells = {};
for (const pdir of readdirSync(roundDir).filter((d) => /^P\d/.test(d))) {
  for (const f of readdirSync(join(roundDir, pdir)).filter((f) => /^D\d.*\.html$/.test(f))) {
    const p = pdir.split('-')[0];
    const d = f.split('-')[0];
    const path = join(roundDir, pdir, f);
    const html = readFileSync(path, 'utf8');
    cells[`${p}${d}`] = {
      href: `${pdir}/${f}`,
      lines: html.split('\n').length,
      kb: Math.round(statSync(path).size / 1024),
      header: parseHeader(html),
    };
  }
}

const P = Object.values(paradigms);
const D = Object.values(designs);
const done = Object.keys(cells).length;

const rows = P.map((p) => `
  <tr>
    <th scope="row" class="rowhead">
      <span class="id">${p.id}</span><span class="name">${esc(p.name)}</span>
      <p>${esc(p.thesis)}</p>
    </th>
    ${D.map((d) => {
      const c = cells[`${p.id}${d.id}`];
      if (!c) return `<td class="cell missing"><span class="id">${p.id}·${d.id}</span><p>not built</p></td>`;
      const th = c.header.thesis.map((t) => `<li>${esc(t)}</li>`).join('');
      return `<td class="cell">
        <a class="open" href="${c.href}" target="_blank" rel="noopener">
          <span class="id">${p.id}·${d.id}</span>
          <span class="title">${esc(p.name)} × ${esc(d.name)}</span>
        </a>
        <ul class="thesis">${th}</ul>
        <span class="meta">${c.lines} lines · ${c.kb} KB</span>
      </td>`;
    }).join('')}
  </tr>`).join('');

const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<title>knobas mockups — ${esc(round)}</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  :root { color-scheme: light dark; --line: #8884; --muted: #888; --accent: #3b6fd6; }
  body { margin: 0; font: 14px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif; padding: 32px 40px; }
  h1 { font-size: 22px; margin: 0 0 4px; }
  h1 small { font-weight: 400; color: var(--muted); margin-left: 8px; }
  .intro { max-width: 80ch; color: var(--muted); margin: 0 0 20px; }
  .intro a { color: var(--accent); }
  table { border-collapse: collapse; width: 100%; table-layout: fixed; }
  th, td { border: 1px solid var(--line); vertical-align: top; padding: 12px 14px; text-align: left; }
  thead th { position: sticky; top: 0; background: Canvas; z-index: 1; }
  thead th .name, .rowhead .name { display: block; font-size: 16px; font-weight: 700; }
  thead th p, .rowhead p { margin: 6px 0 0; font-weight: 400; font-size: 12px; color: var(--muted); }
  .rowhead { width: 15%; }
  .id { font: 600 11px/1 ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--muted); letter-spacing: .04em; display: inline-block; margin-right: 8px; margin-bottom: 4px; }
  .cell { height: 1px; }
  .cell .open { display: block; text-decoration: none; color: inherit; }
  .cell .open:hover .title, .cell .open:focus-visible .title { color: var(--accent); text-decoration: underline; }
  .cell .title { display: block; font-weight: 600; margin-bottom: 6px; }
  .thesis { margin: 0 0 8px; padding-left: 16px; font-size: 12px; color: var(--muted); }
  .thesis li + li { margin-top: 2px; }
  .meta { font-size: 11px; color: var(--muted); font-family: ui-monospace, Menlo, monospace; }
  .missing { background: #8881; color: var(--muted); }
  footer { margin-top: 24px; font-size: 12px; color: var(--muted); }
</style></head><body>
<h1>knobas mockups <small>${esc(round)} · ${done} of ${P.length * D.length} built</small></h1>
<p class="intro">Rows are interaction paradigms, columns are visual designs. Every cell uses the same example data
(<a href="../shared/dataset.md">dataset.md</a>) and must contain the same eight surfaces
(<a href="../shared/screens.md">screens.md</a>). Open a cell, then try <kbd>⌘K</kbd> (search), <kbd>⌘T</kbd> (timer),
and the route PAY-231 → PR #142 → build #1187 → design page → note. Pick by row, by column, or mix ("P3 with D2's look").</p>
<table>
  <thead><tr><th scope="col">paradigm ↓ / design →</th>${D.map((d) => `<th scope="col"><span class="id">${d.id}</span><span class="name">${esc(d.name)}</span><p>${esc(d.thesis)}</p></th>`).join('')}</tr></thead>
  <tbody>${rows}</tbody>
</table>
<footer>Briefs: <a href="../shared/paradigms/">paradigms</a> · <a href="../shared/designs/">designs</a> · <a href="../shared/agent-brief.md">agent brief</a>. Regenerate with <code>node mockups/build-index.mjs ${esc(round)}</code>.</footer>
</body></html>
`;
writeFileSync(join(roundDir, 'index.html'), html);
console.log(`${round}: ${done} cells → ${join(roundDir, 'index.html')}`);
