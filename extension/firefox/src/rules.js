// Which enabled source rule a URL falls under (ADR-0061). The same rule knobas
// applies: exact origin (scheme, host and port; scheme and host lowercased by
// the URL parser), then a path prefix that ends at a segment boundary, the
// longest base path winning when two sources share a host. This is only the
// pre-filter: knobas's `handoff` answer decides what opens.

/** `{ origin, path }` of an http(s) URL, the path without its trailing `/`; null otherwise. */
function parts(raw) {
  let url;
  try {
    url = new URL(raw);
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  return { origin: url.origin, path: url.pathname.replace(/\/+$/, "") };
}

/**
 * The rule whose `base_url` holds `url`, or null.
 * @param {Array<{source_id: string, base_url: string}>} rules
 * @param {string} url
 */
export function matchRule(rules, url) {
  const target = parts(url);
  if (!target) return null;
  let best = null;
  let bestLength = -1;
  for (const rule of rules) {
    const base = parts(rule.base_url);
    if (!base || base.origin !== target.origin) continue;
    const under = target.path === base.path || target.path.startsWith(`${base.path}/`);
    if (under && base.path.length > bestLength) {
      best = rule;
      bestLength = base.path.length;
    }
  }
  return best;
}
