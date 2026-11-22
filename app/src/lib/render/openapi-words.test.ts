import { expect, test } from "vitest";
import { inlineMarkdown, needsBlocks } from "./openapi-view";

test("HTML in a description goes to the sanitizing Markdown renderer; plain words and a line break stay inline", () => {
  // Stripe writes its descriptions in HTML.
  expect(needsBlocks('<p>This method is deprecated. Use <a href="/docs/payments">the Payment Intents API</a>.</p>')).toBe(true);
  expect(needsBlocks("Use <code>amount</code> in cents.")).toBe(true);
  expect(needsBlocks("A line<br>and the next")).toBe(false);
  expect(needsBlocks("Amounts below 1 < 2 are refused")).toBe(false);
  expect(needsBlocks("The `id` of the **payout**.")).toBe(false);
  expect(needsBlocks("- one\n- two")).toBe(true);
  expect(inlineMarkdown("The `id` of the **payout**.")[0]!.map((r) => r.k)).toEqual(["", "code", "", "strong", ""]);
});
