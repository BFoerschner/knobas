/**
 * File pipelines in the viewer (#2264, spec #2234 step 5): the steps a person adds between a file's bytes and its
 * renderer, edited inline in the file viewer (`tf/render/TfPipeline.svelte`) and run in the backend
 * (`entityFilePiped`, `knobas_app::file_pipeline`). This module is the pure half: the *Add step* menu, a step's
 * name, moving a step, and what a run's answer draws as.
 */
import type { CommandStep, NamedPipelineDraft } from "../ipc/app";
import type { PipeFormat, PipeRun, PipeStep } from "../ipc/entity";

/** The decompress step's choices, by the name the step goes by. */
export const DECOMPRESSORS: readonly { step: "gunzip" | "unzstd" | "bunzip2" | "unxz"; label: string; ext: readonly string[] }[] = [
  { step: "gunzip", label: "gzip", ext: ["gz", "gzip", "tgz"] },
  { step: "unzstd", label: "zstd", ext: ["zst", "zstd"] },
  { step: "bunzip2", label: "bzip2", ext: ["bz2", "bz"] },
  { step: "unxz", label: "xz", ext: ["xz"] },
];

export const FORMATS: readonly { format: PipeFormat; label: string }[] = [
  { format: "json", label: "JSON" },
  { format: "yaml", label: "YAML" },
  { format: "toml", label: "TOML" },
];

function extension(path: string): string {
  const base = path.slice(path.lastIndexOf("/") + 1);
  const dot = base.lastIndexOf(".");
  return dot <= 0 ? "" : base.slice(dot + 1).toLowerCase();
}

function formatOf(path: string): PipeFormat | null {
  const ext = extension(path);
  if (ext === "json") return "json";
  if (ext === "yaml" || ext === "yml") return "yaml";
  if (ext === "toml") return "toml";
  return null;
}

/** The name a file goes by after the unwrap steps in `steps`: their extensions taken off, as the backend names it. */
export function nameAfter(path: string, steps: readonly PipeStep[]): string {
  let name = path;
  for (const step of steps) {
    const ext = extension(name);
    const strip = () => name.slice(0, name.length - ext.length - 1);
    const unwraps = DECOMPRESSORS.find((d) => d.step === step.step)?.ext ?? (step.step === "base64_decode" ? ["b64", "base64"] : []);
    if (unwraps.includes(ext)) name = ext === "tgz" ? `${strip()}.tar` : strip();
  }
  return name;
}

/** One row of the *Add step* menu: what it says, what it reads and writes, and the step it adds to a file of `name`. */
export interface StepChoice {
  id: string;
  label: string;
  io: string;
  make: (name: string) => PipeStep;
}

export const STEP_CHOICES: readonly StepChoice[] = [
  { id: "base64_decode", label: "base64 decode", io: "text → bytes", make: () => ({ step: "base64_decode" }) },
  { id: "decompress", label: "gzip · zstd · bzip2 · xz decompress", io: "bytes → bytes", make: (name) => ({ step: decompressorFor(name) }) },
  { id: "jq", label: "jq", io: "JSON → JSON", make: () => ({ step: "jq", filter: "." }) },
  { id: "pretty", label: "Pretty-print JSON · YAML · TOML", io: "text → text", make: (name) => ({ step: "pretty", format: formatOf(name) ?? "json" }) },
  {
    id: "convert",
    label: "Convert JSON ⇄ YAML ⇄ TOML",
    io: "text → text",
    make: (name) => {
      const from = formatOf(name) ?? "json";
      return { step: "convert", from, to: from === "json" ? "yaml" : "json" };
    },
  },
  { id: "base64_encode", label: "base64 encode", io: "bytes → text", make: () => ({ step: "base64_encode" }) },
  { id: "hex_dump", label: "Hex dump", io: "bytes → text", make: () => ({ step: "hex_dump" }) },
];

/** The decompressor a file's extension names; gzip when it names none. */
export function decompressorFor(name: string): (typeof DECOMPRESSORS)[number]["step"] {
  const ext = extension(name);
  return DECOMPRESSORS.find((d) => d.ext.includes(ext))?.step ?? "gunzip";
}

const LABEL: Record<PipeFormat, string> = { json: "JSON", yaml: "YAML", toml: "TOML" };

/** What the bar and a failure call a step: the backend's `PipeStep::name`; a command step by its saved name (#2265). */
export function stepName(step: PipeStep, commands: readonly CommandStep[] = []): string {
  switch (step.step) {
    case "command":
      return commands.find((c) => c.id === step.id)?.name ?? step.id;
    case "base64_decode":
      return "base64 decode";
    case "base64_encode":
      return "base64 encode";
    case "gunzip":
      return "gunzip";
    case "unzstd":
      return "zstd decompress";
    case "bunzip2":
      return "bzip2 decompress";
    case "unxz":
      return "xz decompress";
    case "jq":
      return "jq";
    case "pretty":
      return `pretty-print ${LABEL[step.format]}`;
    case "convert":
      return `${LABEL[step.from]} to ${LABEL[step.to]}`;
    case "hex_dump":
      return "hex dump";
  }
}

/** `steps` with the step at `from` moved to `to`; out of range, unchanged. */
export function moveStep(steps: readonly PipeStep[], from: number, to: number): PipeStep[] {
  const moved = [...steps];
  if (from < 0 || from >= moved.length || to < 0 || to >= moved.length || from === to) return moved;
  const [step] = moved.splice(from, 1);
  moved.splice(to, 0, step!);
  return moved;
}

/** `steps` with the one at `index` replaced. */
export function withStep(steps: readonly PipeStep[], index: number, step: PipeStep): PipeStep[] {
  return steps.map((old, i) => (i === index ? step : old));
}

/** What a finished run draws as: a file of its name, its text, or its bytes' size when they are not text. */
export function pipedFile(run: Extract<PipeRun, { outcome: "done" }>): { path: string; text: string; binary: boolean; size: number } {
  return run.output.kind === "text"
    ? { path: run.name, text: run.output.text, binary: false, size: new TextEncoder().encode(run.output.text).length }
    : { path: run.name, text: "", binary: true, size: run.output.size };
}

/** A size as the failure's *Its input* line says it. */
export function sized(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Where a failed step's input came from: the file, or the step before it. */
export function inputOf(steps: readonly PipeStep[], index: number, commands: readonly CommandStep[] = []): string {
  if (index === 0) return "the file's bytes";
  const before = steps[index - 1];
  return before === undefined ? "the step before" : `the output of step ${index} (${stepName(before, commands)})`;
}

/**
 * The pattern *Save as default* offers for a pipeline on `path` (#2265): its last two extensions, lowercase
 * (`events.json.gz` → `*.json.gz`, so a `.gz` that is not JSON is not taken), or one, or the bare name.
 */
export function pipelinePattern(path: string): string {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const parts = name.split(".");
  if (parts.length < 2 || parts[0] === "") return name;
  return `*.${parts.slice(-Math.min(2, parts.length - 1)).join(".").toLowerCase()}`;
}

/**
 * The named pipelines with `steps` saved as `name` (#2265): the one already called that (any case) changed in
 * place, keeping its id, so every pattern that names it follows; else one more, new.
 */
export function savedAs(
  list: readonly { id: string; name: string; steps: PipeStep[] }[],
  name: string,
  steps: readonly PipeStep[],
): NamedPipelineDraft[] {
  const key = name.trim().toLowerCase();
  const drafts: NamedPipelineDraft[] = list.map((p) => ({ id: p.id, name: p.name, steps: p.steps }));
  const at = drafts.findIndex((p) => p.name.toLowerCase() === key);
  if (at === -1) return [...drafts, { id: null, name: name.trim(), steps: [...steps] }];
  return drafts.map((p, i) => (i === at ? { ...p, steps: [...steps] } : p));
}

/** An unwrap step the viewer offers (never applies) for bytes that look wrapped, and what they look like. */
export interface Suggestion {
  step: PipeStep;
  kind: "gzip-compressed" | "zstd-compressed" | "base64";
}

/** At least this many base64 characters before text is taken for base64. */
const BASE64_AT_LEAST = 16;

/**
 * What a file looks wrapped in (#2265): gzip or zstd by their magic bytes (`bytes`, a binary file's head, `null`
 * when not read), or base64 for text that is nothing else: lines of the base64 alphabet only, padded to a multiple
 * of four, mixing upper case, lower case and digits (hex and plain words don't).
 */
export function suggestion(file: { binary: boolean; text: string }, bytes: Uint8Array | null): Suggestion | null {
  if (file.binary) {
    if (bytes === null) return null;
    if (bytes[0] === 0x1f && bytes[1] === 0x8b) return { step: { step: "gunzip" }, kind: "gzip-compressed" };
    if (bytes[0] === 0x28 && bytes[1] === 0xb5 && bytes[2] === 0x2f && bytes[3] === 0xfd) return { step: { step: "unzstd" }, kind: "zstd-compressed" };
    return null;
  }
  const joined = file.text.trim().replace(/\r?\n/g, "");
  if (joined.length < BASE64_AT_LEAST || joined.length % 4 !== 0) return null;
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(joined)) return null;
  if (!/[A-Z]/.test(joined) || !/[a-z]/.test(joined) || !/[0-9+/]/.test(joined)) return null;
  return { step: { step: "base64_decode" }, kind: "base64" };
}
