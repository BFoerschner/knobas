/**
 * A Java archive on the Code page (ux19): a `.jar`, `.war`, `.ear` or `.aar` opens in the JAR view by default, the
 * archive named by the repository, its path and the commit it was read at; it has no *Source* to switch to, and its
 * bytes never go through the 50 MiB byte read. Where no archive read is wired, it stays "A binary file, not drawn."
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { SourceView } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { plainPort } from "../../render/port";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { jarAnswers } from "../../render/jar-fixture";
import type { ArchivePorts } from "../../render/jar-ports";
import { renderersFor } from "../../render/registry";

const REPO = "gitea:tidewater/payout-service";

/** A source with one binary file at `path`, read at commit `c0mmit`. */
function binarySource(path: string) {
  const name = path.slice(path.lastIndexOf("/") + 1);
  return async (_at: string, section: string | null): Promise<SourceView> => {
    const blocks =
      section === "files"
        ? [{ type: "tree", trail: [], entries: [{ name, kind: "file", size: 184_312, section: `file:${path}` }], truncated: false, url: null }]
        : [{ type: "file", trail: [], path, text: "", language: null, size: 184_312, truncated: false, binary: true, url: null, sha: "b10b", commit: "c0mmit" }];
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

function archivePorts(opened: string[]): ArchivePorts {
  const h = jarAnswers() as unknown as Record<string, (args: Record<string, unknown>) => unknown>;
  const call = <T>(name: string, args: Record<string, unknown>) => Promise.resolve(h[name]!(args) as T);
  return {
    open: (entityId, path, commit) => {
      opened.push(`${entityId} ${path} ${commit}`);
      return call("archive_open", { path: "build/libs/ledger-core-2.3.1.jar" });
    },
    nested: (handle, entry) => call("archive_nested", { handle, entry }),
    entry: (handle, entry) => call("archive_entry", { handle, entry }),
    cls: (handle, entry) => call("archive_class", { handle, entry }),
    decompile: (handle, entry) => call("archive_decompile", { handle, entry }),
    download: (handle, entry) => call("archive_download", { handle, entry }),
    java: () => call("java_runtime", {}),
  };
}

function draw(path: string, archive: ArchivePorts | null, fileBytes = vi.fn(async () => new Uint8Array())) {
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView: binarySource(path),
        fileBytes,
        ...(archive !== null ? { archive } : {}),
        submitWrite: async () => {
          throw { code: "invalid", message: "no write in this suite" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path,
      line: null,
      changed: new Set<string>(),
      treeSections: { dir: "files:", file: "file:" },
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      render: plainPort,
    },
  });
  flushSync();
  return fileBytes;
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

test.each(["build/libs/ledger-core-2.3.1.jar", "dist/shop.war", "ear/app.ear", "android/lib-release.aar"])(
  "%s opens in the JAR view, read at the file's commit, never through the byte read",
  async (path) => {
    const opened: string[] = [];
    const fileBytes = draw(path, archivePorts(opened));
    await vi.waitFor(
      () => {
        flushSync();
        expect(target.querySelector(".tf-jar-name")).not.toBeNull();
      },
      { timeout: 10_000, interval: 20 },
    );
    expect(opened).toEqual([`${REPO} ${path} c0mmit`]);
    expect(fileBytes).not.toHaveBeenCalled();
    expect(text(target.querySelector(".tf-jar-name"))).toBe("ledger-core");
    // A binary file: no Source to switch to, nothing but the JAR view.
    expect(renderersFor({ path, binary: true }).map((r) => r.id)).toEqual(["jar"]);
    expect(target.textContent).not.toContain("A binary file, not drawn.");
  },
);

test("with no archive read wired, a JAR is a binary file, not drawn", async () => {
  draw("build/libs/x.jar", null);
  await vi.waitFor(() => {
    flushSync();
    expect(target.textContent).toContain("A binary file, not drawn.");
  });
});
