// The Pleasant stand-in against the real server's recordings, with no VM up (testenv/pleasant/README.md), and the
// stand-in at volume. Run by `just witness-unit` through testenv/pleasant/test.sh.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { request, token } from "../client.mjs";
import { spawnStandin } from "../contract.mjs";

const HERE = new URL("..", import.meta.url).pathname;

test("the stand-in answers every recorded request as the real server did", () => {
  // contract.mjs exits non-zero and prints each difference when any answer differs
  const out = execFileSync(process.execPath, [`${HERE}contract.mjs`, "--recorded"], { encoding: "utf8" });
  assert.match(out, /contract: all \d+ answers match/);
});

test("at 2,000 entries the stand-in serves a deep tree and folders of 500+", async () => {
  const users = { admin: { user: "admin", password: "knobas-dev" }, writer: { user: "tomasz.wojcik", password: "knobas-dev" }, reader: { user: "mara.lindqvist", password: "knobas-dev" } };
  const s = await spawnStandin(users, ["--entries", "2000"]);
  try {
    const tok = await token(s.base, "mara.lindqvist", "knobas-dev");
    const tree = (await request(s.base, "GET", "/api/v5/rest/folders", { token: tok })).json;
    let entries = 0, deepest = 0, biggest = 0;
    const walk = (f, d) => {
      entries += f.Credentials.length;
      deepest = Math.max(deepest, d);
      biggest = Math.max(biggest, f.Credentials.length);
      for (const c of f.Children) walk(c, d + 1);
    };
    walk(tree, 0);
    assert.equal(entries, 2000);
    assert.ok(deepest >= 8, `deepest folder ${deepest} levels below Root`);
    assert.ok(biggest >= 500, `biggest folder ${biggest} entries`);
    assert.ok(tree.Credentials.every((c) => c.Password === null));
  } finally {
    s.stop();
  }
});
