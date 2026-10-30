/**
 * User command renderers (#2262, spec #2234 step 5, ADR-0033): the programs
 * saved in Settings › Renderers, as registry renderers, and what their output
 * becomes before anything draws it.
 *
 * - **In the registry** each saved renderer is a {@link Renderer} whose id is
 *   `command:<saved id>`, ahead of Source, so it answers `renderersFor` by its
 *   file patterns and `renderersForFence` by its fence languages.
 *   {@link installCommands} replaces the whole set: the shell calls it once
 *   the database answers, Settings › Renderers after every save.
 * - **Running** is the backend's (`run_renderer`): the webview names the
 *   renderer by its saved id and sends the content, never a program. A
 *   {@link CommandPort} is that call, a fake in tests.
 * - **Output** of SVG or HTML goes through DOMPurify, then into a frame with
 *   `sandbox=""` (no scripts, an opaque origin: nothing in it reaches the
 *   app). SVG is drawn inside that frame as an `<img>` of a data URL, which
 *   runs nothing and keeps the SVG's own styles. {@link frameDocument} builds
 *   the frame's document.
 */
import DOMPurify from "dompurify";

import { runRenderer, type CommandOutput, type RenderContent, type RenderRun, type UserRenderer } from "../ipc/app";
import { register, type Renderer } from "./registry";

export const COMMAND_PREFIX = "command:";

/** Runs a saved renderer by id; the app's is the IPC. */
export interface CommandPort {
  run(id: string, content: RenderContent): Promise<RenderRun>;
}

export const ipcCommands: CommandPort = { run: (id, content) => runRenderer(id, content) };

/** The saved renderers installed now, by registry id, and how to take them out again. */
const installed = new Map<string, UserRenderer>();
let undo: (() => void)[] = [];

/** Makes `saved` the command renderers in the registry, in order, replacing the last set. */
export function installCommands(saved: readonly UserRenderer[]): void {
  undo.forEach((step) => step());
  undo = [];
  installed.clear();
  for (const user of saved) {
    const renderer: Renderer = {
      id: `${COMMAND_PREFIX}${user.id}`,
      name: user.name,
      patterns: user.patterns,
      fences: user.fences.map((fence) => fence.toLowerCase()),
      input: user.input,
      output: user.output,
      diff: false,
    };
    undo.push(register(renderer));
    installed.set(renderer.id, user);
  }
}

/** The saved id behind a registry renderer, or `null` for a built-in. */
export function commandIdOf(renderer: Renderer): string | null {
  return renderer.id.startsWith(COMMAND_PREFIX) ? renderer.id.slice(COMMAND_PREFIX.length) : null;
}

/** The saved renderer behind a registry renderer (its command, its timeout), or `null`. */
export function savedCommand(renderer: Renderer): UserRenderer | null {
  return installed.get(renderer.id) ?? null;
}

/** `text` as what the renderer reads: the text itself, or its UTF-8 bytes. */
export function contentFor(renderer: Pick<Renderer, "input">, text: string): RenderContent {
  if (renderer.input === "text") return { kind: "text", text };
  return { kind: "bytes", base64: base64Of(new TextEncoder().encode(text)) };
}

function base64Of(bytes: Uint8Array): string {
  let binary = "";
  for (let at = 0; at < bytes.length; at += 0x8000) binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  return btoa(binary);
}

/**
 * The document a sandboxed frame draws for a run's SVG or HTML `body`,
 * sanitized. An image's `data:` URL needs no frame and is not taken here.
 */
export function frameDocument(output: Exclude<CommandOutput, "image">, body: string): string {
  let inner: string;
  if (output === "html") {
    inner = DOMPurify.sanitize(body, { USE_PROFILES: { html: true, svg: true }, FORBID_TAGS: ["style", "form", "iframe", "object", "embed"] });
  } else {
    const root = DOMPurify.sanitize(body, { USE_PROFILES: { svg: true, svgFilters: true }, RETURN_DOM: true }) as unknown as Element;
    const svg = root.querySelector("svg");
    // XMLSerializer writes the SVG namespace an `<img>` needs.
    const markup = svg === null ? "" : new XMLSerializer().serializeToString(svg);
    inner = `<img alt="" src="data:image/svg+xml;base64,${base64Of(new TextEncoder().encode(markup))}">`;
  }
  return `<!doctype html><html><head><meta charset="utf-8"></head><body>${inner}</body></html>`;
}

/** Why a run drew nothing, in a line, or `null` for a run that drew. */
export function failureLine(run: RenderRun): string | null {
  switch (run.outcome) {
    case "drawn":
      return null;
    case "failed":
      return run.status === null ? "The command was stopped by a signal." : `The command exited with status ${run.status}.`;
    case "timed_out":
      return `The command took longer than its timeout of ${run.seconds} s and was stopped.`;
    case "too_large":
      return `The command wrote more than ${Math.round(run.cap_bytes / (1024 * 1024))} MB and was stopped.`;
    case "not_started":
      return run.reason;
  }
}
