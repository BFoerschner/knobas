// A computed CSS colour read as [r, g, b, alpha], for the probes that ask whether a box paints (scrollers.mjs).
//
// Chromium serializes a computed colour in the space it was made in: `rgb()`/`rgba()` for a hex or rgb colour, but
// `color(srgb r g b / a)` for a `color-mix(in srgb, ...)` and `oklab(L a b / a)` for one `in oklab`. Since app themes
// step 1 (e5e1f432, 2026-11-21) every scrim is a `color-mix()` of `--scrim`, and a reader of `rgb()` alone took the
// open launcher's dimmed window for a layer that draws nothing (the ux30 end gate, `LAYER OVER A LIST div.tf-modal`).
//
// r, g and b are 0..255 for `rgb()`, `rgba()` and `color(srgb ...)`, and NaN in any other space; alpha is 0..1 in every
// space. What is not a colour function reads [0, 0, 0, 0], as `transparent` computes to `rgba(0, 0, 0, 0)`.
//
// The function is written into the page as source (`${rgba}` in scrollers.mjs's AUDIT), so it uses nothing outside it.
export function rgba(s) {
  const m = /^\s*([a-z-]+)\(([^()]*)\)\s*$/i.exec(String(s ?? ""));
  if (!m) return [0, 0, 0, 0];
  const fn = m[1].toLowerCase();
  let body = m[2].trim();
  let space = fn;
  if (fn === "color") {
    const named = /^([a-z0-9-]+)\s+/i.exec(body);
    if (!named) return [0, 0, 0, 0];
    space = named[1].toLowerCase();
    body = body.slice(named[0].length);
  }
  const slash = body.indexOf("/");
  const parts = (slash < 0 ? body : body.slice(0, slash)).split(/[\s,]+/).filter(Boolean);
  const num = (v) => (v.endsWith("%") ? parseFloat(v) / 100 : Number(v));
  const legacy = (fn === "rgb" || fn === "rgba") && slash < 0 && parts.length > 3;
  const alpha = slash >= 0 ? num(body.slice(slash + 1).trim()) : legacy ? num(parts[3]) : 1;
  if (parts.length < 3 || !Number.isFinite(alpha)) return [0, 0, 0, 0];
  const a = Math.min(1, Math.max(0, alpha));
  if (fn === "rgb" || fn === "rgba") return [Number(parts[0]), Number(parts[1]), Number(parts[2]), a];
  if (space === "srgb") return [num(parts[0]) * 255, num(parts[1]) * 255, num(parts[2]) * 255, a];
  return [NaN, NaN, NaN, a];
}
