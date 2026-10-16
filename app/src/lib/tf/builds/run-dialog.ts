/**
 * The Run… dialog's rules (#852, R-9.7), apart from the component so each
 * can be held by a plain test: which fields the parameters are asked with,
 * which branches the combobox offers, what a run sends and the line that
 * says so.
 */
import type { ParameterOption, PipelineSettings, RunPreset } from "../../ipc/entity";
import type { BuildParameter, WriteOpPayload } from "../../ipc/sources";

/** A field as the dialog draws it: text, or a select of fixed values. */
export type FieldKind = { type: "text" } | { type: "select"; options: ParameterOption[] };

/** One declared parameter, with the field it is asked with. */
export interface RunField {
  name: string;
  /** What the source shows instead of the name. */
  label: string | null;
  /** What empty means, or another word from the source about the value. */
  hint: string | null;
  kind: FieldKind;
  /** The value every run starts from; empty for a secret. */
  default: string;
  /** A password: drawn as one, never prefilled, sent only when typed. */
  secret: boolean;
}

/** A free name and value row, from *Add a parameter*. */
export interface ExtraRow {
  name: string;
  value: string;
}

/** A value to start from: a run's parameter or a preset's override. */
export interface StartValue {
  name: string;
  value: string;
  /** A value the source hid (a run's password): never filled in. */
  secret?: boolean;
}

/** The field per parameter, typed by its parsed spec (#852); no spec reads as text. */
export function fieldsOf(settings: PipelineSettings | null): RunField[] {
  const specs = new Map((settings?.parameter_specs ?? []).map((spec) => [spec.name, spec]));
  return (settings?.parameters ?? []).map((parameter) => {
    const spec = specs.get(parameter.name);
    const fallback = parameter.secret ? "" : parameter.value;
    let kind: FieldKind = { type: "text" };
    if (!parameter.secret && spec?.kind.type === "select") kind = { type: "select", options: spec.kind.options };
    if (!parameter.secret && spec?.kind.type === "checkbox") {
      kind = {
        type: "select",
        options: [
          { value: spec.kind.unchecked, label: null },
          { value: spec.kind.checked, label: null },
        ],
      };
    }
    // A default the list does not offer (a value set before the spec was) is
    // offered first, so the field can hold what the pipeline holds.
    if (kind.type === "select" && !kind.options.some((option) => option.value === fallback)) {
      kind = { type: "select", options: [{ value: fallback, label: null }, ...kind.options] };
    }
    return {
      name: parameter.name,
      label: spec?.label ?? null,
      hint: spec?.hint ?? parameter.description,
      kind,
      default: fallback,
      secret: parameter.secret,
    };
  });
}

/** Every field at its default, then `start`'s values for the fields it names. */
export function valuesFrom(fields: readonly RunField[], start: readonly StartValue[]): Record<string, string> {
  const values = Object.fromEntries(fields.map((field) => [field.name, field.default]));
  for (const value of start) {
    if (value.secret || !(value.name in values)) continue;
    values[value.name] = value.value;
  }
  return values;
}

/** One branch the combobox offers, and its note. */
export interface BranchChoice {
  name: string;
  note: "default" | "your job’s branch" | null;
}

/**
 * The combobox's branches: the default first (*default*), then the
 * repository's, then any a run built that the repository did not list, each
 * once. The job's branch says *your job's branch*.
 */
export function branchChoices(from: {
  defaultBranch: string | null;
  repository: readonly string[];
  runs: readonly (string | null)[];
  jobBranch: string | null;
}): BranchChoice[] {
  const names: string[] = [];
  for (const name of [from.defaultBranch, ...from.repository, ...from.runs]) {
    if (name && !names.includes(name)) names.push(name);
  }
  return names.map((name) => ({
    name,
    note: name === from.defaultBranch ? "default" : name === from.jobBranch ? "your job’s branch" : null,
  }));
}

/** The branches holding every typed word, ignoring case; an exact name, or nothing typed, keeps them all. */
export function matchingBranches(choices: readonly BranchChoice[], typed: string): BranchChoice[] {
  const text = typed.trim().toLowerCase();
  if (text === "" || choices.some((choice) => choice.name.toLowerCase() === text)) return [...choices];
  const words = text.split(/\s+/);
  return choices.filter((choice) => words.every((word) => choice.name.toLowerCase().includes(word)));
}

/**
 * What a run sends (`RunBuild.parameters`): each field whose value differs
 * from its default, in the fields' order, then each added row with a name.
 * A secret's default is never drawn, so it is sent only when typed.
 */
export function overridesOf(fields: readonly RunField[], values: Record<string, string>, extra: readonly ExtraRow[]): BuildParameter[] {
  const out: BuildParameter[] = [];
  for (const field of fields) {
    const value = values[field.name] ?? field.default;
    if (value !== field.default) out.push({ name: field.name, value });
  }
  for (const row of extra) {
    const name = row.name.trim();
    if (name !== "") out.push({ name, value: row.value });
  }
  return out;
}

/** The footer's line: *On feature/x, with test.filter=sepa::retry*, or *On main, defaults*. */
export function summaryOf(branch: string, overrides: readonly BuildParameter[]): string {
  if (overrides.length === 0) return `On ${branch}, defaults`;
  return `On ${branch}, with ${overrides.map((p) => `${p.name}=${p.value === "" ? "(empty)" : p.value}`).join(", ")}`;
}

/** The run options the dialog offers under the source's name. */
export interface RunOptions {
  clean_checkout: boolean;
  first_in_queue: boolean;
}

/**
 * The write a run queues. The default branch, by name or left empty, runs as
 * no branch: TeamCity reads an explicit default branch's name as a branch
 * that is not its default one (`run_action` in the adapter's view).
 */
export function runPayload(
  entity: string,
  typed: string,
  defaultBranch: string | null,
  parameters: BuildParameter[],
  options: RunOptions,
): WriteOpPayload {
  const name = typed.trim();
  const branch = name === "" || name === defaultBranch ? null : name;
  return { RunBuild: { entity, branch, parameters, ...options } };
}

/** What *Run* hands the page: the write, the line that says it, and a preset to save with it. */
export interface RunRequest {
  payload: WriteOpPayload;
  summary: string;
  preset: RunPreset | null;
}
