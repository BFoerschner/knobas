/**
 * What *Create monitor for this asset* opens on (issue #453, spec #427 story
 * 70's *"prefilled from the asset's route or hostname"*).
 *
 * A unit file rather than more assertions through the dialog, because the
 * derivation has five branches and a rendered form can witness one at a time.
 * The dialog's own tests are in `AssetsView.monitor.test.svelte.ts` and are
 * about the two writes it makes.
 */
import { expect, test } from "vitest";

import type {
  AssetDetail,
  AssetProperty,
  AssetRow,
  RouteRow,
} from "../ipc/assets";
import { prefillFor, urlFor } from "./create-monitor";

const ASSET: AssetRow = {
  id: "asset:knobas-gitea",
  parent_id: "asset:orbstack-docker",
  type_id: "container",
  type_label: "Container",
  monogram: "CT",
  name: "knobas-gitea",
  status: "none",
  environment: "dev",
  owner: null,
  has_children: false,
  health: "none",
  inside: "none",
  problems_inside: 0,
  linked_work: 0,
};

function route(name: string, url: string): RouteRow {
  return {
    id: `route:${name}`,
    asset_id: ASSET.id,
    asset_name: ASSET.name,
    target_id: null,
    target_name: null,
    name,
    url,
    visibility: "internal",
    properties: [],
  };
}

function text(key: string, value: string): AssetProperty {
  return { key, label: key, value: { kind: "text", value }, custom: false };
}

function detail(over: Partial<AssetDetail> = {}): AssetDetail {
  return {
    asset: ASSET,
    properties: [],
    effective_environment: null,
    effective_owner: null,
    held_by: [],
    holds: [],
    exposes: [],
    reachable_via: [],
    history: [],
    links: [],
    monitors: [],
    monitoring: [],
    monitor_targets: [],
    ...over,
  };
}

/** A route the asset exposes is the strongest claim, so it wins. */
test("an exposed route is what the form opens on", () => {
  expect(
    urlFor(
      detail({
        exposes: [route("Gitea", "http://127.0.0.1:3000/")],
        reachable_via: [route("elsewhere", "http://127.0.0.1:9999/")],
        properties: [text("hostname", "gitea.invalid")],
      }),
    ),
  ).toBe("http://127.0.0.1:3000/");
});

/**
 * A route that *reaches* the asset is the container-behind-a-published-port
 * case, and it is the real estate's: `route:notebook-gitea` is exposed by the
 * notebook and lands on the Gitea container.
 */
test("a route that reaches the asset is next", () => {
  expect(
    urlFor(
      detail({
        reachable_via: [route("Gitea", "http://127.0.0.1:3000/")],
        properties: [text("hostname", "gitea.invalid")],
      }),
    ),
  ).toBe("http://127.0.0.1:3000/");
});

/**
 * **A route Kuma could not fetch is not a URL this form may open with**, and
 * this is the direction that matters: an estate's routes are as often
 * `postgres://` and `ssh://` as they are HTTP, the adapter refuses them by
 * name, and a form that opened on one would put a create in front of a reader
 * that could only ever be refused.
 */
test("a route on a scheme an HTTP check cannot fetch is skipped", () => {
  expect(
    urlFor(
      detail({
        exposes: [
          route("Postgres", "postgres://127.0.0.1:5432/knobas"),
          route("SSH", "ssh://127.0.0.1:22"),
          route("Gitea", "https://127.0.0.1:3000/"),
        ],
      }),
    ),
  ).toBe("https://127.0.0.1:3000/");
  // And with nothing fetchable at all, the form opens empty rather than on a
  // URL Kuma would report down for ever.
  expect(
    urlFor(detail({ exposes: [route("SSH", "ssh://127.0.0.1:22")] })),
  ).toBe("");
});

/** The hostname, then the IP — the guesses, in that order. */
test("the hostname is the guess, and the IP is the weaker one", () => {
  expect(
    urlFor(detail({ properties: [text("hostname", "vm-db-01.invalid")] })),
  ).toBe("http://vm-db-01.invalid/");
  // Loopback, which `house-rules`' network scan exempts and every other
  // address in this repository's fixtures is: the assertion is about *which*
  // property was read, not about which address.
  expect(urlFor(detail({ properties: [text("ip", "127.0.0.1")] }))).toBe(
    "http://127.0.0.1/",
  );
  // Both: the hostname wins, because an IP is what a host answers to and a
  // hostname is what somebody named it.
  expect(
    urlFor(
      detail({
        properties: [text("ip", "127.0.0.1"), text("hostname", "db.invalid")],
      }),
    ),
  ).toBe("http://db.invalid/");
});

/**
 * An asset with nothing to guess from opens empty, and a blank property is
 * nothing to guess from.
 *
 * The blank case is the one worth pinning: `""` is a value the pane can hold
 * for a declared key nobody filled in, and composing a URL out of it would
 * give `http:///` — which parses, which Kuma would accept, and which watches
 * nothing.
 */
test("an asset with no route and no host opens empty", () => {
  expect(urlFor(detail())).toBe("");
  expect(urlFor(detail({ properties: [text("hostname", "   ")] }))).toBe("");
  expect(
    urlFor(
      detail({
        properties: [
          { key: "hostname", label: "Hostname", value: null, custom: false },
        ],
      }),
    ),
  ).toBe("");
  // A `number` where a `hostname` should be is not a host either: the read is
  // of the declared shape, not a coercion of whatever was stored.
  expect(
    urlFor(
      detail({
        properties: [
          {
            key: "hostname",
            label: "Hostname",
            value: { kind: "number", value: 8080 },
            custom: false,
          },
        ],
      }),
    ),
  ).toBe("");
});

/**
 * The name is the asset's own, and that is load-bearing rather than lazy: the
 * name is what `knobas_sync::attach` matches the mirror's monitor titles
 * against.
 */
test("the name the form opens on is the asset's", () => {
  expect(prefillFor(detail())).toEqual({ name: "knobas-gitea", url: "" });
});
