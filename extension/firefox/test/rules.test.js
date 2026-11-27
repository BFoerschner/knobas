import assert from "node:assert/strict";
import { test } from "node:test";
import { matchRule } from "../src/rules.js";

const jira = { source_id: "jira", base_url: "https://jira.tidewater.example", scope: "known_items" };
const wiki = { source_id: "wiki", base_url: "https://wiki.tidewater.example/confluence", scope: "whole_source" };
const wikiEng = { source_id: "wiki-eng", base_url: "https://wiki.tidewater.example/confluence/eng", scope: "known_items" };
const rules = [jira, wiki, wikiEng];

test("a URL under a rule's base URL matches that rule", () => {
  assert.equal(matchRule(rules, "https://jira.tidewater.example/browse/PAY-231")?.source_id, "jira");
  assert.equal(matchRule(rules, "https://jira.tidewater.example")?.source_id, "jira");
  assert.equal(matchRule(rules, "https://wiki.tidewater.example/confluence/display/ENG/Home")?.source_id, "wiki");
});

test("the path prefix ends at a segment boundary", () => {
  assert.equal(matchRule(rules, "https://wiki.tidewater.example/confluencex/page"), null);
  assert.equal(matchRule(rules, "https://wiki.tidewater.example/"), null);
  assert.equal(matchRule(rules, "https://wiki.tidewater.example/confluence?x=1")?.source_id, "wiki");
});

test("the longest base path wins when two rules share a host", () => {
  assert.equal(matchRule(rules, "https://wiki.tidewater.example/confluence/eng/pages/1")?.source_id, "wiki-eng");
  assert.equal(matchRule(rules, "https://wiki.tidewater.example/confluence/engine")?.source_id, "wiki");
});

test("scheme, host and port must match exactly; case of scheme and host does not count", () => {
  assert.equal(matchRule(rules, "http://jira.tidewater.example/browse/PAY-1"), null);
  assert.equal(matchRule(rules, "https://jira.tidewater.example:8443/browse/PAY-1"), null);
  assert.equal(matchRule(rules, "https://JIRA.Tidewater.example/browse/PAY-1")?.source_id, "jira");
  assert.equal(matchRule(rules, "https://jira.tidewater.example.evil.test/browse/PAY-1"), null);
  const ported = [{ source_id: "tc", base_url: "http://ci.example:8111", scope: "known_items" }];
  assert.equal(matchRule(ported, "http://ci.example:8111/viewLog.html?buildId=4")?.source_id, "tc");
  assert.equal(matchRule(ported, "http://ci.example/viewLog.html?buildId=4"), null);
});

test("anything but http(s), and rules with a broken base URL, match nothing", () => {
  assert.equal(matchRule(rules, "ftp://jira.tidewater.example/browse/PAY-1"), null);
  assert.equal(matchRule(rules, "not a url"), null);
  assert.equal(matchRule([{ source_id: "x", base_url: "nonsense", scope: "known_items" }], "https://x.example/"), null);
});
