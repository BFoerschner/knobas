import { expect, test, vi } from "vitest";

import { loadAhead } from "./doc-module";

/** A `later` the test opens by hand. */
function gate() {
  let open = () => {};
  const later = () => new Promise<void>((resolve) => (open = resolve));
  return { later, open: () => open() };
}

test("a view's code loads ahead once the reader is still, and once a session however many pages ask", async () => {
  const load = vi.fn();
  const { later, open } = gate();
  loadAhead("test-once", load, later);
  loadAhead("test-once", load, later);
  await Promise.resolve();
  expect(load, "nothing before the reader is still").not.toHaveBeenCalled();
  open();
  await vi.waitFor(() => expect(load).toHaveBeenCalledTimes(1));
  loadAhead("test-once", load, () => Promise.resolve());
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(load).toHaveBeenCalledTimes(1);
});

test("a load that failed is tried again by the next page that asks", async () => {
  const load = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(undefined);
  loadAhead("test-again", load, () => Promise.resolve());
  await vi.waitFor(() => expect(load).toHaveBeenCalledTimes(1));
  await new Promise((resolve) => setTimeout(resolve, 0));
  loadAhead("test-again", load, () => Promise.resolve());
  await vi.waitFor(() => expect(load).toHaveBeenCalledTimes(2));
});
