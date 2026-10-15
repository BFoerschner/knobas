import { expect, test } from "vitest";

import { parseTfHash, sectionOf } from "./route";

test("the empty address is Tickets, as in prototype-6", () => {
  expect(parseTfHash("")).toEqual({ view: "tickets", rest: [] });
  expect(parseTfHash("#/")).toEqual({ view: "tickets", rest: [] });
  expect(parseTfHash("#/tickets/v-mine")).toEqual({ view: "tickets", rest: ["v-mine"] });
});

test("a job carries its key and its tab", () => {
  expect(parseTfHash("#/job/PAY-231/conv")).toEqual({ view: "job", key: "PAY-231", tab: "conv" });
  expect(parseTfHash("#/job/PAY-231")).toEqual({ view: "job", key: "PAY-231", tab: null });
  expect(parseTfHash("#/job")).toEqual({ view: "job", key: null, tab: null });
});

test("a deep address lights the section it belongs to", () => {
  expect(sectionOf(parseTfHash("#/pr/139"))).toBe("code");
  expect(sectionOf(parseTfHash("#/build/payout/1187"))).toBe("builds");
  expect(sectionOf(parseTfHash("#/page/4711"))).toBe("docs");
  expect(sectionOf(parseTfHash("#/asset/vm-pay-01"))).toBe("deps");
  expect(sectionOf(parseTfHash("#/review"))).toBe("review");
  expect(sectionOf(parseTfHash("#/settings/source/jira"))).toBe("settings");
});

test("a rooms-UI address is unknown here, not Tickets", () => {
  expect(parseTfHash("#/ctx/all")).toEqual({ view: "unknown", hash: "#/ctx/all" });
  expect(sectionOf(parseTfHash("#/ticket/jira:PAY-231"))).toBeNull();
});
