import { expect, test } from "vitest";
import { fakeTicketCount } from "./fake/corpus";

// The fixture's restatement of the grammar the server reads (#1903, #1889).
test("a quoted phrase is one needle: it needs the words side by side", () => {
  expect(fakeTicketCount('"payout batch"')).toBe(1);
  expect(fakeTicketCount('"batch payout"')).toBe(0);
});

test("-word and -\"phrase\" leave out, they are not matched literally", () => {
  const payout = fakeTicketCount("payout");
  expect(fakeTicketCount("payout -export")).toBe(payout - fakeTicketCount("payout export"));
  expect(fakeTicketCount("payout -export")).toBeGreaterThan(0);
  expect(fakeTicketCount('payout -"payout batch"')).toBe(payout - 1);
});

test("or separates alternatives and binds looser than the words", () => {
  const credentials = fakeTicketCount("credentials");
  const webhooks = fakeTicketCount("webhooks");
  expect(fakeTicketCount("credentials or webhooks")).toBe(credentials + webhooks);
  expect(fakeTicketCount("credentials or")).toBe(credentials);
  expect(fakeTicketCount("pay-244 rotate or webhooks")).toBe(webhooks);
});

test("a key stays a filter beside the words", () => {
  expect(fakeTicketCount("payout assignee:jonas")).toBe(fakeTicketCount("payout") - fakeTicketCount("payout -assignee:jonas"));
});
