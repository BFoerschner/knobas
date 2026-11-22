/**
 * An OpenAPI spec on the Code page (ux18): it opens in the OpenAPI view by default, whatever it is called, with
 * *Source* one pick away in *Render as*; other YAML stays Source; a ref into another file is read at the spec's
 * commit through the byte read; an address names an operation inside a spec (`at/<place>/<path>`).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { SourceView } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { plainPort } from "../../render/port";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { COMPOSE_YAML, ORDER_YAML, PETS_30_YAML, SPLIT_31_YAML } from "../../render/openapi-fixture";
import { knowsRepoTab, placeHash, readRepoRoute } from "./repo";

const REPO = "gitea:tidewater/payout-service";

/** A source with one text file at `path`, read at commit `c0mmit`. */
function fileSource(path: string, text: string) {
  const name = path.slice(path.lastIndexOf("/") + 1);
  return async (_at: string, section: string | null): Promise<SourceView> => {
    const blocks =
      section === "files"
        ? [{ type: "tree", trail: [], entries: [{ name, kind: "file", size: text.length, section: `file:${path}` }], truncated: false, url: null }]
        : [{ type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: null, sha: "b10b", commit: "c0mmit" }];
    return { sections: [{ id: section, title: "Files", count: null, deferred: false, blocks }], actions: [] } as unknown as SourceView;
  };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(path: string, text: string, fileBytes?: (entityId: string, path: string, commit: string | null) => Promise<Uint8Array>, place: string | null = null) {
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView: fileSource(path, text),
        ...(fileBytes ? { fileBytes } : {}),
        submitWrite: async () => {
          throw { code: "invalid", message: "no write in this suite" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path,
      line: null,
      place,
      changed: new Set<string>(),
      treeSections: { dir: "files:", file: "file:" },
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      render: plainPort,
    },
  });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 10; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

/** Until the view's chunk has loaded (the first import compiles it under Vitest) and the spec is read. */
async function specDrawn() {
  await vi.waitFor(
    () => {
      flushSync();
      expect(target.querySelector(".tf-oas, .tf-oas-bad")).not.toBeNull();
    },
    { timeout: 10_000, interval: 20 },
  );
  await settle();
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const face = () => text(target.querySelector(".tf-fileview .tf-rsw-face"));

test("a spec opens in the OpenAPI view by default, at any name; Source is one pick away", async () => {
  draw("api/v1/petstore.oas.yml", PETS_30_YAML);
  await specDrawn();
  expect(face()).toBe("Render as API");
  expect(target.querySelector(".tf-fileview")!.classList.contains("tf-fv-oas")).toBe(true);
  expect(text(target.querySelector(".tf-oas-title"))).toBe("Pet Store");
  expect(target.querySelector(".tf-fileview .tf-code")).toBeNull();

  target.querySelector<HTMLButtonElement>(".tf-rsw-face")!.click();
  flushSync();
  const items = [...target.querySelectorAll<HTMLElement>(".tf-rsw .tf-menu [role^='menuitem']")];
  expect(items.map((item) => text(item).replace(/^✓ ?/, "")).slice(0, 2).map((t) => t.split(" ")[0])).toEqual(["API", "Source"]);
  items.find((item) => text(item).replace(/^✓ ?/, "").startsWith("Source"))!.click();
  await settle();
  expect(face()).toBe("Render as Source");
  expect(target.querySelector(".tf-oas")).toBeNull();
  expect(target.querySelector(".tf-fileview .tf-code")).not.toBeNull();
});

test("a YAML file that is no spec stays Source and offers no OpenAPI", async () => {
  draw("docker-compose.yml", COMPOSE_YAML);
  await settle();
  expect(face()).toBe("Render as Source");
  expect(target.querySelector(".tf-oas")).toBeNull();
});

test("a spec whose version key is past the sniff's head opens as a spec (ux31)", async () => {
  const schemas: Record<string, unknown> = {};
  for (let i = 0; i < 1300; i++) schemas[`S${i}`] = { type: "object", description: "x".repeat(220) };
  draw("api/sorted.json", JSON.stringify({ components: { schemas }, info: { title: "Sorted", version: "1" }, openapi: "3.0.3", paths: {} }));
  await specDrawn();
  expect(face()).toBe("Render as API");
  expect(text(target.querySelector(".tf-oas-title"))).toBe("Sorted");
});

test("a large file whose openapi key is nested is offered, parsed, and drawn as Source with no error (ux31)", async () => {
  draw("package.json", JSON.stringify({ name: "pkg", config: { tools: { openapi: "3.0.0" } }, blob: "y".repeat(300 * 1024) }));
  await vi.waitFor(
    () => {
      flushSync();
      expect(face()).toBe("Render as Source");
    },
    { timeout: 10_000, interval: 20 },
  );
  await settle();
  expect(target.querySelector(".tf-oas-bad")).toBeNull();
  expect(target.querySelector(".tf-fileview .tf-code")).not.toBeNull();
});

test("a ref into another file is read at the spec's commit through the byte read, only when asked", async () => {
  const reads: string[] = [];
  draw("api/spec.yaml", SPLIT_31_YAML, async (entityId, path, commit) => {
    reads.push(`${entityId} ${path} ${commit}`);
    return new TextEncoder().encode(ORDER_YAML);
  });
  await specDrawn();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-oas-row")].find((b) => text(b) === "GET /orders")!.click();
  await settle();
  expect(reads).toEqual([]);
  // The response names the ref in a row of its own; its caret reads the other file, only when pressed.
  const media = [...target.querySelectorAll<HTMLElement>(".tf-oas-page .tf-oas-sec")].find((s) => text(s.querySelector("h3")).startsWith("Responses"))!;
  expect(text(media)).toContain("Order");
  await settle();
  expect(reads).toEqual([]);
  media.querySelector<HTMLButtonElement>(".tf-oas-tog")!.click();
  await vi.waitFor(() => {
    flushSync();
    expect(reads).toEqual([`${REPO} api/schemas/order.yaml c0mmit`]);
  });
  await settle();
  expect([...media.querySelectorAll(".tf-oas-fname")].map((el) => text(el))).toContain("id");
});

test("an address names a place inside a file: an operation opens there", async () => {
  const hash = placeHash(REPO, "api/openapi.yaml", "get /pets/{petId}");
  expect(hash).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/at/get%20%2Fpets%2F%7BpetId%7D/api/openapi.yaml");
  const rest = hash.slice("#/repo/".length).split("/").map(decodeURIComponent);
  expect(knowsRepoTab(rest)).toBe(true);
  expect(readRepoRoute(rest)).toMatchObject({ tab: "files", path: "api/openapi.yaml", place: "get /pets/{petId}", line: null });

  draw("api/openapi.yaml", PETS_30_YAML, undefined, "get /pets/{petId}");
  await specDrawn();
  const row = [...target.querySelectorAll<HTMLButtonElement>(".tf-oas-row")].find((b) => text(b) === "GET /pets/{petId}")!;
  expect(row.getAttribute("aria-current")).toBe("true");
  expect(text(target.querySelector(".tf-oas-page .tf-oas-h"))).toBe("GET /pets/{petId}");
});
