import { afterEach, expect, test } from "vitest";

import { sessionFilter } from "../filter/filter-state.svelte";
import { scopedTicket } from "./scoped.svelte";

afterEach(() => scopedTicket.clear());

test("a card's Builds row turns This job on for the rail and the runs alike", () => {
  sessionFilter("builds-pipelines").scope = "all";
  sessionFilter("builds-runs").scope = "all";
  scopedTicket.open("builds", "mock:PAY-231");
  expect(sessionFilter("builds-pipelines").scope).toBe("job");
  expect(sessionFilter("builds-runs").scope).toBe("job");
});
