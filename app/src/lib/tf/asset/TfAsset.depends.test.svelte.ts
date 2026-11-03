/**
 * *Depends on this* — the panel on an asset's page that answers *what breaks
 * if this goes down* (issue #505, spec §12.2, `CONTEXT.md`'s **Depends on
 * this**, whose own entry warns off calling the panel a blast radius: that is
 * the question it answers, not its name). It moved with the rest of the tree's
 * pane to the page (`TfAsset.svelte`).
 *
 * A file of its own for `TfAsset.monitor.test.svelte.ts`' reason: this
 * needs a port whose answer moves between tests and which records *which*
 * asset it was asked about, and mixing it into the read file would make those
 * assertions depend on which test ran first.
 *
 * **What is witnessed here is the panel, not the walk.** Whether the closure
 * is right — descendants, both relations, a cycle walked once, the routes left
 * out of the count — is `crates/knobas-app/tests/it/assets_ipc.rs` over a real
 * PostgreSQL and the real estate file. What is this file's is the half a
 * reader touches: the count is the assets', the path and the relation are on
 * every line, the routes are beneath and outside the number, and the panel
 * never shows one asset's answer under another's heading.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { AssetDetail, AssetRow, DependsOnThis, RouteRow } from "../../ipc/assets";
import { createTestNav } from "../../assets/test-nav.svelte";
import Estate from "../../assets/Estate.fixture.svelte";
import { waitFor } from "../../shell/test-wait";

const NOW = new Date(2026, 8, 8, 12, 0, 0, 0);

function row(id: string, name: string, parent: string | null, type = "container"): AssetRow {
  return {
    id,
    parent_id: parent,
    type_id: type,
    type_label: type === "vm" ? "VM" : "Container",
    monogram: type === "vm" ? "VM" : "CT",
    name,
    status: "none",
    environment: null,
    owner: null,
    has_children: false,
    health: "none",
    inside: "none",
    problems_inside: 0,
    linked_work: 0,
  };
}

const GITEA = row("asset:knobas-gitea", "knobas-gitea", null);
const TEAMCITY = row("asset:hetzner-teamcity", "knobas-teamcity", null, "vm");
const AGENT = row("asset:knobas-teamcity-agent", "knobas-teamcity-agent", null);
const ENGINE = row("asset:hetzner-teamcity-docker", "Docker engine (knobas-teamcity)", null);

const ESTATE = [GITEA, TEAMCITY, AGENT, ENGINE];

const TUNNEL: RouteRow = {
  id: "route:notebook-gitea",
  asset_id: "asset:notebook",
  asset_name: "devs-MacBook-Pro",
  target_id: GITEA.id,
  target_name: GITEA.name,
  name: "Gitea",
  url: "http://127.0.0.1:3000/",
  visibility: "internal",
  properties: [],
};
// The real estate file's own reverse tunnel resolves the docker network name
// `gitea`, which is a hostname that is neither loopback nor reserved and is
// therefore an offence in `src/**` (`house-rules.test.ts`, *no runtime network
// references*). The file is data and is exempt; a fixture spelled by hand is
// not, so this one takes a reserved name. What is under test is that a second
// route is drawn beneath the count, and the host is not part of that.
const REVERSE: RouteRow = {
  ...TUNNEL,
  id: "route:tunnel-gitea-reverse",
  asset_id: TEAMCITY.id,
  asset_name: TEAMCITY.name,
  name: "gitea (reverse tunnel)",
  url: "http://gitea.test:3000/",
};

/** What the panel answers for one asset, and nothing for the others. */
type Answers = Record<string, DependsOnThis | { refusal: { code: string; message: string } }>;

const NOTHING: DependsOnThis = { assets: [], routes: [] };

/**
 * The bridge, over an estate of four flat assets.
 *
 * The tree is flat on purpose: the *columns* are not what this file is about,
 * and every relation on screen comes from the panel's own answer rather than
 * from what holds what.
 */
function bridge(answers: Answers) {
  const asked: string[] = [];
  /**
   * Set by {@link holdNext}: the next walk answers this promise's resolver and
   * nothing else, so a test can stand in the gap between two selections.
   *
   * A flag inside the closure rather than a function swapped on the object
   * afterwards, because the page copies its ports once at init — a port
   * replaced on this object after mount is a port the page never reads.
   */
  let holding: ((answer: DependsOnThis) => void) | null = null;
  let hold = false;
  return {
    asked,
    /** Hold the next walk open, and hand back the release. */
    holdNext() {
      hold = true;
      return (answer: DependsOnThis) => {
        holding?.(answer);
        holding = null;
      };
    },
    assetTypes: () => Promise.resolve([]),
    assetTree: (parentId?: string | null) =>
      Promise.resolve((parentId ?? null) === null ? ESTATE : []),
    getAsset: (assetId: string): Promise<AssetDetail> => {
      const asset = ESTATE.find((candidate) => candidate.id === assetId);
      if (asset === undefined) {
        return Promise.reject({ code: "not_found", message: `no asset ${assetId}` });
      }
      return Promise.resolve({
        asset,
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
      });
    },
    dependsOnThis: (assetId: string): Promise<DependsOnThis> => {
      asked.push(assetId);
      if (hold) {
        hold = false;
        return new Promise<DependsOnThis>((resolve) => {
          holding = resolve;
        });
      }
      const answer = answers[assetId] ?? NOTHING;
      if ("refusal" in answer) return Promise.reject(answer.refusal);
      return Promise.resolve(answer);
    },
    openExternal: () => Promise.resolve(),
    // A route's address reads its route before it knows the asset; this estate
    // has none to read, and the one test that follows a route only reads the address.
    getRoute: (routeId: string) => Promise.reject({ code: "not_found", message: `no route ${routeId}` }),
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(hash: string, answers: Answers) {
  location.hash = hash;
  const nav = createTestNav();
  const ports = bridge(answers);
  app = mount(Estate, {
    target,
    props: { nav, now: () => NOW, ports: ports as never },
  });
  flushSync();
  return { nav, ports };
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** The number beside the heading, or `null` while the read is out or when nothing breaks. */
function count(): string | null {
  return target.querySelector(".tf-as-breaks .tf-as-n")?.textContent?.trim() ?? null;
}

/** Every line of the panel, whitespace collapsed, in the order it is drawn. */
function lines(): string[] {
  return [...target.querySelectorAll(".tf-as-breaks li")].map((line) =>
    (line.textContent ?? "").replace(/\s+/g, " ").trim(),
  );
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app !== undefined) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
});

/**
 * Story 51 and story 54 on screen: how many break, and, per line, where it
 * sits and why it is there.
 *
 * The three lines are the three ways onto the list — a `depends-on` link, a
 * `runs-on` link, and containment — and each is read in the vocabulary's own
 * words rather than as the stored key: `depends-on` is a key and *depends on*
 * is a sentence. The null is the tree, which is a `parent_id` field and not a
 * link (ADR-0014), and it reads *inside*.
 */
test("the panel counts what breaks and says where each one sits and why", async () => {
  render("#/asset/asset:knobas-gitea", {
    [GITEA.id]: {
      assets: [
        { asset: TEAMCITY, path: "knobas test estate / Hetzner Cloud nbg1", relation: "depends-on" },
        { asset: AGENT, path: "knobas test estate / knobas-teamcity", relation: "runs-on" },
        { asset: ENGINE, path: null, relation: null },
      ],
      routes: [],
    },
  });

  await waitFor(() => expect(text()).toContain("Depends on this"));
  expect(count()).toBe("3");
  expect(lines()).toEqual([
    "knobas-teamcity depends on knobas test estate / Hetzner Cloud nbg1",
    "knobas-teamcity-agent runs on knobas test estate / knobas-teamcity",
    "Docker engine (knobas-teamcity) inside",
  ]);
});

/**
 * Story 52: the routes are beneath the list and **outside** the number.
 *
 * Both halves are asserted, because a panel that dropped the routes would also
 * satisfy a count of one: the number says *1*, and the two routes are on
 * screen under a line of their own that names what they are.
 */
test("the routes that would break are listed beneath and are not in the count", async () => {
  render("#/asset/asset:knobas-gitea", {
    [GITEA.id]: {
      assets: [{ asset: TEAMCITY, path: null, relation: "depends-on" }],
      routes: [TUNNEL, REVERSE],
    },
  });

  await waitFor(() => expect(count()).toBe("1"));
  expect(text()).toContain("Reachable via routes that would break");
  expect(lines()).toEqual([
    "knobas-teamcity depends on",
    "Gitea http://127.0.0.1:3000/",
    "gitea (reverse tunnel) http://gitea.test:3000/",
  ]);
});

/**
 * *Nothing* is an answer, and the panel gives it.
 *
 * The section is drawn on every asset: "what breaks if I turn this off" is a
 * question about every asset, and the reader about to pull a machine out is
 * asking it precisely when the answer is nothing. The sentence says it, so the
 * heading carries no count: a count badge is drawn only above zero (the shell
 * conventions' *Copy and display*).
 */
test("an asset nothing depends on says so, and draws no count", async () => {
  render("#/asset/asset:knobas-gitea", { [GITEA.id]: NOTHING });

  await waitFor(() => expect(text()).toContain("Nothing depends on this."));
  expect(count()).toBeNull();
  expect(lines()).toEqual([]);
});

/**
 * The panel never shows one asset's answer under another's heading.
 *
 * The second asset's read is left pending, so the moment under test is the one
 * a slow walk really produces: the page has moved and the answer has not. What
 * must be on screen then is *this* asset's absence of an answer, not the
 * previous one's list — a panel naming four machines under a container that
 * holds none would be the page stating a falsehood.
 *
 * The count is asserted as well as the lines: a guard that cleared the list
 * but left the number would still read "3" over an empty panel.
 */
test("opening another asset asks again and never draws the last one's answer", async () => {
  const { nav, ports } = render("#/asset/asset:knobas-gitea", {
    [GITEA.id]: {
      assets: [{ asset: TEAMCITY, path: null, relation: "depends-on" }],
      routes: [TUNNEL],
    },
  });
  await waitFor(() => expect(count()).toBe("1"));

  // The second walk is held open, which is what makes the assertions below
  // about the gap between the two answers rather than about a race.
  const release = ports.holdNext();
  nav.go("#/asset/asset:hetzner-teamcity");
  flushSync();
  await waitFor(() => expect(ports.asked).toContain(TEAMCITY.id));
  expect(lines()).toEqual([]);
  expect(count()).toBeNull();
  expect(text()).not.toContain("knobas-teamcity depends on");
  expect(text()).toContain("Reading…");

  release({ assets: [{ asset: AGENT, path: null, relation: "runs-on" }], routes: [] });
  await waitFor(() => expect(count()).toBe("1"));
  expect(lines()).toEqual(["knobas-teamcity-agent runs on"]);
});

/**
 * A refused read says so, rather than drawing the empty state.
 *
 * The two are different sentences — *nothing depends on this* and *knobas
 * could not work out what does* — and a panel that drew the first for the
 * second would tell a reader a machine was safe to turn off. The page's own
 * read succeeded here, which is the case that makes this visible at all: the
 * asset is on screen and only this section is short.
 */
test("a refused walk draws its message and not the empty state", async () => {
  render("#/asset/asset:knobas-gitea", {
    [GITEA.id]: { refusal: { code: "internal", message: "the estate could not be walked" } },
  });

  await waitFor(() => expect(text()).toContain("the estate could not be walked"));
  expect(text()).not.toContain("Nothing depends on this.");
  expect(count()).toBeNull();
  // The page itself is fine: this failure is the panel's alone.
  expect(target.querySelector("h1")?.textContent?.trim()).toBe("knobas-gitea");
});

/** A line is one click to the page of the asset it names, which is what makes it useful. */
test("a line on the panel opens the page of the asset it names", async () => {
  const { nav } = render("#/asset/asset:knobas-gitea", {
    [GITEA.id]: {
      assets: [{ asset: TEAMCITY, path: null, relation: "depends-on" }],
      routes: [],
    },
  });
  await waitFor(() => expect(count()).toBe("1"));

  const line = [...target.querySelectorAll<HTMLButtonElement>(".tf-as-breaks li button")].find(
    (candidate) => (candidate.textContent ?? "").trim() === TEAMCITY.name,
  );
  expect(line).toBeDefined();
  line?.click();
  flushSync();

  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: TEAMCITY.id });
  await waitFor(() => expect(target.querySelector("h1")?.textContent?.trim()).toBe(TEAMCITY.name));
});

/**
 * #1606: a route name too long for the column wrapped centred (a button
 * centres its text) and pushed its URL right. jsdom does no layout, so the
 * rule is pinned in the page's stylesheet: the name is left-aligned, the row
 * wraps, and the URL takes a line of its own beneath the name, in mono with
 * no ligatures.
 */
test("a route's name is left-aligned and its URL wraps beneath it (#1606)", async () => {
  const fs = await import("node:fs");
  const css = fs.readFileSync("src/lib/tf/css/asset.css", "utf8");
  const tokens = fs.readFileSync("src/lib/tf/css/tokens.css", "utf8");
  const page = fs.readFileSync("src/lib/tf/asset/TfAsset.svelte", "utf8");
  const rule = (sheet: string, selector: string) =>
    sheet.match(new RegExp(`(?:^|\\n)${selector.replace(/[.]/g, "\\.")}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
  expect(rule(css, ".tf-as-rname, .tf-as-url, .tf-as-to")).toMatch(/text-align:\s*left/);
  expect(rule(css, ".tf-as-list > li")).toMatch(/display:\s*flex/);
  expect(rule(css, ".tf-as-wrap > li")).toMatch(/flex-wrap:\s*wrap/);
  expect(rule(css, ".tf-as-wrap .tf-as-path")).toMatch(/flex-basis:\s*100%/);
  expect(rule(tokens, ".tf .tf-mono")).toMatch(/font-variant-ligatures:\s*none/);
  // The breaking routes' list wraps, and each URL is the mono path under its name.
  expect(page).toMatch(/Reachable via routes that would break<\/h4>\s*<ul class="tf-as-list tf-as-wrap">/);
  expect(page).toMatch(/onclick=\{\(\) => go\(routeAddressOf\(route\)\)\}>\{route\.name\}<\/button>\s*<span class="tf-dim tf-mono tf-as-path">\{route\.url\}<\/span>/);
});

/**
 * A route on the panel opens its own address, which draws the page of the
 * asset exposing it with the route picked.
 */
test("a route that would break opens the route's address", async () => {
  const { nav } = render("#/asset/asset:knobas-gitea", {
    [GITEA.id]: { assets: [{ asset: TEAMCITY, path: null, relation: "depends-on" }], routes: [TUNNEL] },
  });
  await waitFor(() => expect(count()).toBe("1"));

  const route = [...target.querySelectorAll<HTMLButtonElement>(".tf-as-breaks li button")].find(
    (candidate) => (candidate.textContent ?? "").trim() === TUNNEL.name,
  );
  expect(route).toBeDefined();
  route?.click();
  flushSync();

  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: null, routeId: TUNNEL.id });
});

/**
 * #2210: a closure of thousands draws through `Windowed`, the lines near the
 * viewport and the count of all of them.
 */
test("a long Depends on this list draws the lines near the viewport and counts them all (#2210)", async () => {
  const many = Array.from({ length: 2000 }, (_, at) => ({ asset: row(`asset:bulk-${at}`, `bulk-${at}`, null), path: null, relation: null }));
  render("#/asset/asset:knobas-gitea", { [GITEA.id]: { assets: many, routes: [] } });

  await waitFor(() => expect(count()).toBe("2000"));
  expect(lines().length).toBeGreaterThan(5);
  expect(lines().length).toBeLessThan(100);
  expect(lines()[0]).toBe("bulk-0 inside");
});
