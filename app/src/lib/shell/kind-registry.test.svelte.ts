/**
 * §3a's claim, on the frontend: *"a new source's items get grouped, chipped
 * and labeled in the launcher without touching core"*.
 *
 * The registry is what makes that literally true for the shell — the only
 * layer that can be right about a kind knobas has never seen — so what is
 * pinned here is that a declaration wins over knobas' own vocabulary, that an
 * undeclared kind still renders, and that two adapters declaring one kind id
 * resolve the same way every time rather than by whichever answered first.
 */
import { expect, test, vi } from "vitest";

import type { SourceDescriptor } from "../ipc/sources";
import { createKindRegistry } from "./kind-registry.svelte";
import { kindLabel, kindMonogram, kindSingular } from "./kinds";

function descriptor(adapterKind: string, kinds: SourceDescriptor["entity_kinds"]): SourceDescriptor {
  return {
    id: adapterKind,
    adapter_kind: adapterKind,
    name: adapterKind,
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: [],
    write_ops: [],
    payload_paths: [],
    entity_kinds: kinds,
    config_schema: { type: "object", properties: {} },
  };
}

const JIRA = descriptor("jira", [
  { id: "ticket", label: "Issue", plural: "Issues", monogram: "IS", full_sync_exhaustive: false },
]);

const QUOKKA = descriptor("quokka", [
  {
    id: "incident",
    label: "Incident",
    plural: "Incidents",
    monogram: "IN",
    full_sync_exhaustive: true,
  },
  { id: "ticket", label: "Case", plural: "Cases", monogram: "CS", full_sync_exhaustive: false },
]);

function registry(adapters: SourceDescriptor[]) {
  return createKindRegistry({ listAdapters: () => Promise.resolve(adapters) });
}

test("a declared kind uses the adapter's label, plural and monogram", async () => {
  const kinds = registry([JIRA]);
  await kinds.load();

  const info = kinds.info("ticket");
  expect(info).toMatchObject({ label: "Issue", plural: "Issues", monogram: "IS" });
  // ...and the helpers take it, so the whole shell reads the adapter's word
  // rather than knobas' own. `ticket` is in knobas' vocabulary as *Ticket*, so
  // this fixture can tell a declaration apart from the fallback.
  expect(kindSingular("ticket", info)).toBe("Issue");
  expect(kindLabel("ticket", info)).toBe("Issues");
  expect(kindMonogram("ticket", info)).toBe("IS");
});

test("an undeclared kind still gets a usable label and monogram", async () => {
  const kinds = registry([JIRA]);
  await kinds.load();

  expect(kinds.info("incident")).toBeNull();
  // §3a's fallback is the designed path, not a degradation: knobas may not
  // carry a table of every kind every adapter will ever emit.
  expect(kindLabel("incident", kinds.info("incident"))).toBe("Incidents");
  expect(kindMonogram("incident", kinds.info("incident"))).toBe("IN");
});

test("two adapters declaring the same kind id do not fight — first wins, deterministically", async () => {
  const kinds = registry([JIRA, QUOKKA]);
  await kinds.load();
  // `list_adapters` answers from a fixed table in a fixed order, so "first"
  // is a stable answer rather than a race.
  expect(kinds.info("ticket")?.label).toBe("Issue");

  const reversed = registry([QUOKKA, JIRA]);
  await reversed.load();
  expect(reversed.info("ticket")?.label).toBe("Case");
  // Whatever the order, the *same* registry answers the same way every time.
  expect(reversed.info("ticket")?.label).toBe("Case");
});

test("a caller that knows the adapter gets that adapter's declaration, not the first", async () => {
  const kinds = registry([JIRA, QUOKKA]);
  await kinds.load();
  // A Quokka ticket is a Case. The global lookup cannot know that; a caller
  // holding the source's `adapter_kind` can, and this is the seam that keeps
  // the detail header from labelling it with another adapter's word.
  expect(kinds.infoFor("quokka", "ticket")?.label).toBe("Case");
  expect(kinds.infoFor("jira", "ticket")?.label).toBe("Issue");
  expect(kinds.infoFor("nobody", "ticket")).toBeNull();
});

test("the registry is asked once, because a descriptor is static per build", async () => {
  let asked = 0;
  const kinds = createKindRegistry({
    listAdapters: () => {
      asked += 1;
      return Promise.resolve([JIRA]);
    },
  });
  await Promise.all([kinds.load(), kinds.load(), kinds.load()]);
  await kinds.load();
  expect(asked).toBe(1);
});

test("a registry that could not be read answers null rather than throwing at a tile", async () => {
  const kinds = createKindRegistry({
    listAdapters: () => Promise.reject({ code: "internal", message: "no registry", source_id: null }),
  });
  await kinds.load();
  expect(kinds.info("ticket")).toBeNull();
  // The shell still draws: every caller's fallback is §3a's humaniser.
  expect(kindLabel("ticket", kinds.info("ticket"))).toBe("Tickets");
});

test("a failed load can be retried, unlike a successful one", async () => {
  let asked = 0;
  const kinds = createKindRegistry({
    listAdapters: () => {
      asked += 1;
      return asked === 1 ? Promise.reject(new Error("not ready")) : Promise.resolve([JIRA]);
    },
  });
  await kinds.load();
  expect(kinds.info("ticket")).toBeNull();
  // `list_adapters` answers before bring-up, but it can still fail on the
  // first frames before Tauri has injected its internals. Caching *that* would
  // leave the shell with no kind metadata for the whole session.
  await kinds.load();
  expect(kinds.info("ticket")?.label).toBe("Issue");
  expect(asked).toBe(2);
});

test("the registry is reactive, so a tile drawn before it loaded redraws", async () => {
  const kinds = registry([JIRA]);
  const seen: (string | null)[] = [];
  const stop = $effect.root(() => {
    $effect(() => {
      seen.push(kinds.info("ticket")?.label ?? null);
    });
  });
  await kinds.load();
  await vi.waitFor(() => expect(seen).toContain("Issue"));
  // Null first — the shell draws before `list_adapters` answers — then the
  // declaration. A non-reactive registry would leave the first reading on
  // screen for ever.
  expect(seen[0]).toBeNull();
  stop();
});
