// A small Pleasant Password Server REST client for the seed and the contract (API v5, testenv/pleasant/README.md).
// The real server answers HTTPS with its own self-signed certificate, so certificate checks are off for it here,
// and only here: this talks to a disposable test VM on 127.0.0.1.
import http from "node:http";
import https from "node:https";

const insecure = new https.Agent({ rejectUnauthorized: false, keepAlive: true });
const plain = new http.Agent({ keepAlive: true });

/** One raw request: status, the headers (lower-case names), the body as text and parsed JSON (or null), the time taken. */
export function request(base, method, path, { body, form, token, headers = {} } = {}) {
  const url = new URL(path, base);
  const h = { ...headers };
  let data = null;
  if (form) { data = new URLSearchParams(form).toString(); h["content-type"] = "application/x-www-form-urlencoded"; }
  else if (body !== undefined) { data = JSON.stringify(body); h["content-type"] = "application/json"; }
  if (data !== null) h["content-length"] = Buffer.byteLength(data);
  if (token) h.authorization = `Bearer ${token}`;
  const mod = url.protocol === "https:" ? https : http;
  const t0 = performance.now();
  return new Promise((resolve, reject) => {
    const req = mod.request(url, { method, headers: h, agent: url.protocol === "https:" ? insecure : plain, timeout: 300_000 }, (res) => {
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => {
        const text = Buffer.concat(chunks).toString("utf8");
        let json = null;
        try { json = text ? JSON.parse(text) : null; } catch { json = null; }
        resolve({ status: res.statusCode, headers: res.headers, text, json, ms: performance.now() - t0 });
      });
    });
    req.on("timeout", () => req.destroy(new Error(`${method} ${path}: timed out`)));
    req.on("error", reject);
    if (data !== null) req.write(data);
    req.end();
  });
}

export async function token(base, username, password) {
  const r = await request(base, "POST", "/OAuth2/Token", { form: { grant_type: "password", username, password } });
  if (r.status !== 200) throw new Error(`token for ${username}: HTTP ${r.status} ${r.text.slice(0, 300)}`);
  return r.json.access_token;
}

/** An API v5 client that throws on anything but 2xx. */
export function client(base, tok) {
  const call = async (method, path, body) => {
    const r = await request(base, method, `/api/v5/rest/${path}`, { body, token: tok });
    if (r.status < 200 || r.status > 299) throw new Error(`${method} ${path}: HTTP ${r.status} ${r.text.slice(0, 400)}`);
    return r.json;
  };
  return { call, get: (p) => call("GET", p), post: (p, b) => call("POST", p, b) };
}
