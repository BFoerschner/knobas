import { describe, expect, test } from "vitest";

import type { ParameterSpec, PipelineSettings, Setting } from "../../ipc/entity";
import {
  branchChoices,
  fieldsOf,
  matchingBranches,
  overridesOf,
  runPayload,
  summaryOf,
  valuesFrom,
  type ExtraRow,
  type RunField,
} from "./run-dialog";

const own = (name: string, value: string, over: Partial<Setting> = {}): Setting => ({
  name,
  value,
  inherited: false,
  secret: false,
  editable: true,
  description: null,
  ...over,
});

/** prototype-6's `paramDefs` for Payout_IntegrationTests, as the settings read answers them. */
const SETTINGS: PipelineSettings = {
  repositories: [{ name: "payout-service", url: null, default_branch: "refs/heads/main", entity_id: null }],
  triggers: [],
  parameters: [own("env", "staging"), own("test.filter", ""), own("log.level", "info"), own("deploy.key", "", { secret: true })],
  parameter_write_op: "set_parameter",
  parameter_specs: [
    { name: "env", label: null, hint: null, kind: { type: "select", options: [{ value: "staging", label: null }, { value: "prod-dryrun", label: null }] } },
    { name: "test.filter", label: null, hint: "empty runs every test", kind: { type: "text" } },
    {
      name: "log.level",
      label: null,
      hint: null,
      kind: {
        type: "select",
        options: [
          { value: "info", label: null },
          { value: "debug", label: "Debug" },
        ],
      },
    },
    { name: "deploy.key", label: null, hint: null, kind: { type: "text" } },
  ] satisfies ParameterSpec[],
  chain: [],
  steps: [],
};

describe("fieldsOf", () => {
  test("a field per parameter, typed by its spec, with its default and hint", () => {
    const fields = fieldsOf(SETTINGS);
    expect(fields.map((f) => [f.name, f.kind.type, f.default, f.hint])).toEqual([
      ["env", "select", "staging", null],
      ["test.filter", "text", "", "empty runs every test"],
      ["log.level", "select", "info", null],
      ["deploy.key", "text", "", null],
    ]);
    expect(fields[3]!.secret).toBe(true);
  });

  test("a parameter with no spec (an older peer) is text", () => {
    const fields = fieldsOf({ ...SETTINGS, parameter_specs: [] });
    expect(fields.every((f) => f.kind.type === "text")).toBe(true);
  });

  test("a checkbox is a select of its two values", () => {
    const fields = fieldsOf({
      ...SETTINGS,
      parameters: [own("dry.run", "false")],
      parameter_specs: [{ name: "dry.run", label: null, hint: null, kind: { type: "checkbox", checked: "true", unchecked: "false" } }],
    });
    expect(fields[0]!.kind).toEqual({
      type: "select",
      options: [
        { value: "false", label: null },
        { value: "true", label: null },
      ],
    });
  });

  test("a default the select does not offer is still offered, so the field can hold it", () => {
    const fields = fieldsOf({ ...SETTINGS, parameters: [own("env", "qa")], parameter_specs: [SETTINGS.parameter_specs[0]!] });
    expect(fields[0]!.kind.type === "select" && fields[0]!.kind.options.map((o) => o.value)).toEqual(["qa", "staging", "prod-dryrun"]);
  });
});

describe("branches", () => {
  test("the default first, marked; then the repository's and the runs' branches, each once; the job's marked", () => {
    const choices = branchChoices({
      defaultBranch: "main",
      repository: ["main", "feature/PAY-231-sepa-retry", "release/1.9"],
      runs: ["feature/csv-export", "main", "feature/PAY-231-sepa-retry"],
      jobBranch: "feature/PAY-231-sepa-retry",
    });
    expect(choices).toEqual([
      { name: "main", note: "default" },
      { name: "feature/PAY-231-sepa-retry", note: "your job’s branch" },
      { name: "release/1.9", note: null },
      { name: "feature/csv-export", note: null },
    ]);
  });

  test("type-ahead keeps the branches holding every typed word; an exact name shows them all", () => {
    const choices = branchChoices({ defaultBranch: "main", repository: ["feature/PAY-231-sepa-retry", "feature/csv-export"], runs: [], jobBranch: null });
    expect(matchingBranches(choices, "pay sepa").map((c) => c.name)).toEqual(["feature/PAY-231-sepa-retry"]);
    expect(matchingBranches(choices, "main").map((c) => c.name)).toEqual(["main", "feature/PAY-231-sepa-retry", "feature/csv-export"]);
    expect(matchingBranches(choices, "").length).toBe(3);
    expect(matchingBranches(choices, "nothing")).toEqual([]);
  });
});

const FIELDS: RunField[] = fieldsOf(SETTINGS);
const DEFAULTS = valuesFrom(FIELDS, []);

describe("what runs", () => {
  test("only what differs from its default is sent, in the fields' order, then the added rows with a name", () => {
    const values = { ...DEFAULTS, "log.level": "debug", "test.filter": "sepa::retry" };
    const extra: ExtraRow[] = [
      { name: "env.EXTRA", value: "1" },
      { name: "  ", value: "dropped" },
    ];
    expect(overridesOf(FIELDS, values, extra)).toEqual([
      { name: "test.filter", value: "sepa::retry" },
      { name: "log.level", value: "debug" },
      { name: "env.EXTRA", value: "1" },
    ]);
  });

  test("a secret is sent only when one was typed", () => {
    expect(overridesOf(FIELDS, DEFAULTS, [])).toEqual([]);
    expect(overridesOf(FIELDS, { ...DEFAULTS, "deploy.key": "k" }, [])).toEqual([{ name: "deploy.key", value: "k" }]);
  });

  test("the summary line says the branch and what changed, or defaults", () => {
    expect(summaryOf("feature/PAY-231-sepa-retry", [{ name: "test.filter", value: "sepa::retry" }])).toBe(
      "On feature/PAY-231-sepa-retry, with test.filter=sepa::retry",
    );
    expect(summaryOf("main", [])).toBe("On main, defaults");
    expect(summaryOf("main", [{ name: "x", value: "" }])).toBe("On main, with x=(empty)");
  });

  test("the default branch, by name or left empty, runs as no branch", () => {
    const options = { clean_checkout: true, first_in_queue: false };
    expect(runPayload("tc:buildType:X", "main", "main", [], options)).toEqual({
      RunBuild: { entity: "tc:buildType:X", branch: null, parameters: [], clean_checkout: true, first_in_queue: false },
    });
    expect(runPayload("tc:buildType:X", "  ", "main", [], options)).toMatchObject({ RunBuild: { branch: null } });
    expect(runPayload("tc:buildType:X", " feature/x ", "main", [], options)).toMatchObject({ RunBuild: { branch: "feature/x" } });
    expect(runPayload("tc:buildType:X", "main", null, [], options)).toMatchObject({ RunBuild: { branch: "main" } });
  });
});

describe("start from", () => {
  test("a run's or a preset's values over the defaults; one the fields do not name is ignored", () => {
    expect(
      valuesFrom(FIELDS, [
        { name: "env", value: "prod-dryrun" },
        { name: "system.teamcity.version", value: "2026.1" },
      ]),
    ).toEqual({ ...DEFAULTS, env: "prod-dryrun" });
  });

  test("a secret the run hid stays as its default, never the empty value drawn", () => {
    expect(valuesFrom(FIELDS, [{ name: "deploy.key", value: "", secret: true }])["deploy.key"]).toBe("");
    expect(overridesOf(FIELDS, valuesFrom(FIELDS, [{ name: "deploy.key", value: "", secret: true }]), [])).toEqual([]);
  });
});
