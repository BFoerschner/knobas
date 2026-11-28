/**
 * `?fake-ipc`: *Try it on* (#2562), on every renderer's, command's and application's settings page.
 *
 * - `plugin:dialog|open`, the native picker, is stubbed: a file picks `/Users/mara/examples/order-flow.md`, a folder
 *   `/Users/mara/examples/flows`, so a shot or a test can press *Choose a file…* with no window to answer.
 *   `?fake-try-pick=none` answers as a picker closed with nothing chosen.
 * - `try_bytes` answers the stub file's bytes (and two more examples by their path), refuses a folder and a path it
 *   does not know as the backend refuses one gone; `try_piped` runs the fake's steps over them or over pasted text.
 * - `try_folder_render`, `oneoff_try` and `open_try` answer as `folder_render`, `oneoff_run` and `open_folder` do, on
 *   the entity `~try`, the chosen folder (or a throwaway one) in the checkout's place.
 */
import type { FolderRenderView, TryOn } from "../../../ipc/app";
import type { OneOffTarget } from "../../../ipc/oneoff";
import type { PipeStep } from "../../../ipc/entity";
import type { Handler } from "../fake-tauri";
import { handlers as folderRenders } from "./folder-render";
import { handlers as oneoffs } from "./oneoff";
import { fakeValuePiped } from "./views/repos";

type Handlers = Record<string, Handler>;

const enc = (text: string) => new TextEncoder().encode(text);

/** The examples on the fake's disk, by path. */
const FILES: Record<string, string> = {
  "/Users/mara/examples/order-flow.md": "# Order flow\n\nAn order is **placed**, then *paid*, then shipped.\n\n```mermaid\ngraph LR\n  placed --> paid --> shipped\n```\n",
  "/Users/mara/examples/flow.mmd": "graph TD\n  placed --> paid\n  paid --> shipped\n",
  "/Users/mara/examples/order.json": '{"order":{"id":"A-1001","lines":[{"sku":"BOLT-6","qty":40}]}}\n',
};
const FOLDER = "/Users/mara/examples/flows";

/** What a chosen path stands for: its last name, and its folder. */
const leaf = (path: string) => path.slice(path.lastIndexOf("/") + 1);
const parent = (path: string) => path.slice(0, path.lastIndexOf("/")) || "/";

function bytesOf(on: TryOn): { name: string; bytes: Uint8Array } {
  if (on.kind === "text") {
    if (on.name.trim() === "" || on.name.includes("/")) throw { code: "invalid", message: "Give the text a file name, such as example.md.", source_id: null };
    return { name: on.name.trim(), bytes: enc(on.text) };
  }
  if (on.path === FOLDER) throw { code: "invalid", message: `${on.path} is a folder: choose a file`, source_id: null };
  const text = FILES[on.path];
  if (text === undefined) throw { code: "not_found", message: `${on.path} is no longer there`, source_id: null };
  return { name: leaf(on.path), bytes: enc(text) };
}

export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  const closed = params.get("fake-try-pick") === "none";
  return {
    "plugin:dialog|open": (args) => {
      if (closed) return null;
      const options = (args["options"] ?? {}) as { directory?: boolean };
      return options.directory ? FOLDER : "/Users/mara/examples/order-flow.md";
    },
    try_bytes: (args) => bytesOf({ kind: "path", path: String(args["path"]) }).bytes,
    try_piped: (args) => {
      const { bytes } = bytesOf(args["on"] as TryOn);
      return fakeValuePiped(new TextDecoder().decode(bytes), (args["steps"] as PipeStep[] | undefined) ?? []);
    },
    try_folder_render: (args) => {
      const on = args["on"] as TryOn;
      if (on.kind === "path" && on.path !== FOLDER && FILES[on.path] === undefined) {
        return { renderer: String(args["renderer"]), folder: "", drawn: null, current: false, running: null, failure: { why: "copy", reason: `${on.path} is no longer there` }, from: "working_tree", commit: null } satisfies FolderRenderView;
      }
      return folderRenders().folder_render!({ entityId: "~try", renderer: args["renderer"], folder: "", commit: null, paths: null, ask: args["ask"], lane: "try" });
    },
    oneoff_try: (args) => {
      const on = args["on"] as TryOn;
      const command = String(args["command"]);
      const throwaway = "/var/folders/tmp/knobas-try/command-1";
      const root = on.kind === "text" ? throwaway : on.path === FOLDER ? FOLDER : parent(on.path);
      const file = on.kind === "text" ? on.name : on.path === FOLDER ? null : leaf(on.path);
      const target: OneOffTarget = file !== null ? { kind: "file", path: file, lines: null, at: null } : { kind: "folder", path: "" };
      const started = oneoffs().oneoff_run!({ entityId: "~try", command, target, madeToken: null, channel: args["channel"] }) as { cwd: string };
      return { ...started, cwd: root };
    },
    open_try: (args) => {
      const on = args["on"] as TryOn;
      const at = on.kind === "text" ? `/var/folders/tmp/knobas-try/open-1/${on.name}` : on.path;
      return ["code", at];
    },
  };
}
