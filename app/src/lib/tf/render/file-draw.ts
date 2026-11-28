/**
 * **Which leaf draws a file** (#2581): the one dispatch the Code page's file view and Settings' *Try it on* share, so
 * a renderer added here draws in both. `TfFileDraw.svelte` holds the branch for each route.
 *
 * The order is the Code page's: what reads bytes first (a PDF, a binary image), then the JAR view, then a binary file
 * nothing draws, then what draws text.
 */
import type { ComponentProps } from "svelte";

import { commandIdOf } from "../../render/command";
import type { Renderer } from "../../render/registry";
import type { Elsewhere } from "../../render/xsd-locate";
import type TfDocView from "./doc/TfDocView.svelte";
import type { JarPorts } from "./TfJar.svelte";

type DocProps = ComponentProps<typeof TfDocView>;

/** A leaf of the file view. */
export const FILE_ROUTES = ["pdf", "image", "jar", "jar-elsewhere", "binary", "svg", "openapi", "xsd", "doc", "markdown", "html", "diagram", "command", "lines"] as const;
export type FileRoute = (typeof FILE_ROUTES)[number];

/**
 * Where the archive reads a JAR opens with stand: the repository's (`read`); a repository that has none, where the JAR
 * is a binary file not drawn (`unread`); or no repository at all (`outside`, *Try it on*), where it says so.
 */
export type ArchiveReads = "read" | "unread" | "outside";

/**
 * The leaf that draws `drawn` as `renderer` draws it. `bytes` is whether the file's bytes, as read, stand beside it
 * (never for a pipeline's output).
 */
export function routeOf(renderer: Renderer | null, drawn: { binary: boolean } | null, bytes: boolean, archive: ArchiveReads): FileRoute {
  const as = renderer?.id ?? null;
  if (bytes && as === "pdf") return "pdf";
  if (bytes && drawn !== null && drawn.binary && as === "image") return "image";
  if (as === "jar" && archive !== "unread") return archive === "read" ? "jar" : "jar-elsewhere";
  if (drawn === null || drawn.binary) return "binary";
  if (as === "image") return "svg";
  if (as === "openapi") return "openapi";
  if (as === "xsd") return "xsd";
  if (as === "xml" || as === "json") return "doc";
  if (as === "markdown") return "markdown";
  if (as === "html") return "html";
  if (as === "mermaid" || as === "d2" || as === "graphviz") return "diagram";
  if (renderer !== null && commandIdOf(renderer) !== null) return "command";
  return "lines";
}

/**
 * The Code page's repository, as the file view's leaves read and move in it. *Try it on* has none, and the parts that
 * need one stay off: relative links and images, the explorers' places in the address, the XML view's schema check and
 * jq, the JAR view.
 */
export interface FileDrawRepo {
  /** The repository: what the views keep their state by. */
  readonly id: string;
  /** The entity the file is read from: what Markdown's image sizes are kept by. */
  readonly entity: string;
  /** The file as read (not a pipeline's output): Markdown's base, the JAR's path, its blob. */
  readonly file: { readonly path: string; readonly sha: string | null };
  /** Whether `drawn` is the file as read, not a pipeline's output. */
  asRead(drawn: object): boolean;
  /** What the address names: a file, a place in it, a line. */
  readonly path: string | null;
  readonly place: string | null;
  readonly line: number | null;
  /** Another file's bytes at the commit shown; `null` where none can be read. */
  readonly images: ((path: string) => Promise<Uint8Array>) | null;
  /** Another file as text at the commit shown. */
  readonly text: (path: string) => Promise<string | null>;
  readonly paths: () => Promise<readonly string[]>;
  readonly readOther: (repo: string, path: string) => Promise<string | null>;
  readonly elsewhere: (namespace: string) => Promise<Elsewhere[]>;
  /** The JAR view's archive reads; `null` where the repository has none. */
  readonly jar: JarPorts | null;
  /** Hrefs: a file at a line, a file at a place, a file, a file at a place in the address. */
  readonly fileLink: (to: string, at: number | null) => string;
  readonly placeLink: (to: string, at: string | null) => string;
  readonly hrefOf: (path: string) => string;
  readonly lineHrefOf: (path: string, line: number) => string;
  readonly placeHrefOf: (path: string, at: string) => string;
  /** Moves: to another place (a pushed entry), the address replaced, Back. */
  go(hash: string, label: string): void;
  replace(hash: string): void;
  /** *Copy link* on a place in a file. */
  copyPlace(path: string, at: string, label: string): void;
  /** The file shown as Source. */
  source(path: string): void;
  /** The XML view's reads of the repository and its schema check (`TfDocView`'s `files` and `schema`). */
  readonly files: NonNullable<DocProps["files"]>;
  readonly schema: NonNullable<DocProps["schema"]>;
  /** jq on a JSON file: the file as the backend reads it where `drawn` is the file as read, else the text drawn. */
  query(drawn: { path: string; text: string }): Promise<number>;
  /** *Use as this file's pipeline*: the filter added to the file's pipeline as a jq step. */
  pipeline(path: string, filter: string): void;
  /** A value's pipeline run and save, for the file as read; `null` where none runs. */
  readonly values: ((path: string) => NonNullable<DocProps["values"]>) | null;
}
