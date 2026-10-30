/**
 * File pipelines in the viewer (#2264, spec #2234 step 5): the steps a person adds between a file's bytes and its
 * renderer, edited inline in the file viewer (`tf/render/TfPipeline.svelte`) and run in the backend
 * (`entityFilePiped`, `knobas_app::file_pipeline`). This module is the pure half: the *Add step* menu, a step's
 * name, moving a step, and what a run's answer draws as.
 */
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

/** What the bar and a failure call a step: the backend's `PipeStep::name`. */
export function stepName(step: PipeStep): string {
  switch (step.step) {
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
export function inputOf(steps: readonly PipeStep[], index: number): string {
  if (index === 0) return "the file's bytes";
  const before = steps[index - 1];
  return before === undefined ? "the step before" : `the output of step ${index} (${stepName(before)})`;
}
