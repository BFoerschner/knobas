/**
 * What the launcher treats as a pasted link rather than a query (#496,
 * spec #491 story 9).
 *
 * The seam is the function the session calls before it sends anything: a
 * string in, the URL or `null` out. Both directions matter and the second one
 * matters more — every one of the launcher's own grammars is a string a
 * careless recogniser would swallow, and a swallowed query is a search box
 * that has stopped searching.
 */
import { expect, test } from "vitest";

import { pastedUrl } from "./url";

test("a link from chat is recognised, whatever it picked up on the way", () => {
  for (const link of [
    "https://jira.example/browse/PAY-231",
    "http://jira.example/browse/PAY-231",
    "https://confluence.example/pages/viewpage.action?pageId=98307",
    "https://gitea.example/acme/payments-svc/pulls/142#issuecomment-9",
    "https://kuma.example/dashboard/8/",
    "https://JIRA.example:8443/browse/PAY-231",
  ]) {
    expect(pastedUrl(link), link).toBe(link);
  }
});

test("surrounding whitespace is a paste, not a query", () => {
  expect(pastedUrl("  https://jira.example/browse/PAY-231\n")).toBe(
    "https://jira.example/browse/PAY-231",
  );
});

test("every launcher grammar is still a query", () => {
  // The five prefixes the backend parses (`?` help, `>` actions, `#` kinds,
  // `@` people, `/` source), plus ordinary words and a bare entity key.
  for (const query of [
    "",
    "   ",
    "sepa retry",
    "?",
    ">go to settings",
    "#tickets",
    "@bjoern",
    "/jira",
    "PAY-231",
    "jira.example/browse/PAY-231",
    "/browse/PAY-231",
  ]) {
    expect(pastedUrl(query), JSON.stringify(query)).toBeNull();
  }
});

test("a query that merely contains a link is a query", () => {
  // `new URL` percent-encodes an inner space, so this parses; a reader typing
  // words around a link is searching, not pasting.
  expect(pastedUrl("retry https://jira.example/browse/PAY-231")).toBeNull();
  expect(pastedUrl("https://jira.example/browse/PAY-231 retry")).toBeNull();
});

test("a scheme the browser would refuse is not a paste", () => {
  // `openExternal` allows `http:`/`https:` and nothing else, so recognising
  // any of these would end in a refusal the reader cannot act on.
  for (const other of [
    "file:///Users/bjoern/notes.md",
    "vscode://file/tmp/x",
    "mailto:bjoern@example.com",
    "javascript:alert(1)",
    "ms-msdt:/id",
    "http:PAY-231",
  ]) {
    expect(pastedUrl(other), other).toBeNull();
  }
});
