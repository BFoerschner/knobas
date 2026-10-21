/**
 * The generated form: one control per `SchemaField`, wired so the browser, the
 * assistive layer and the validator all agree about the same field.
 *
 * `schema-form.test.ts` pins the *model*. What is here only exists once there
 * is a DOM: that an enum renders as a select and not as a text box, that a
 * list round-trips through a textarea without losing entries, that a message
 * lands next to the field it is about, and that an adapter which needs no
 * configuration says so instead of showing an empty box.
 *
 * Carried from the rooms `sources/SchemaForm.test.svelte.ts` (#1506). Fields
 * are found by the control id the form draws (`<idPrefix>-<key>`).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { EMPTY_SCHEMA, JIRA_SCHEMA, TEAMCITY_SCHEMA } from "../../sources/fixtures";
import { defaultValues, schemaFields, validate } from "../../sources/schema-form";
import TfSchemaForm from "./TfSchemaForm.svelte";

const PREFIX = "sf";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(schema: unknown, errors: Record<string, string> = {}, layout: "rows" | "grid" = "grid") {
  const fields = schemaFields(schema);
  const values = $state(defaultValues(fields));
  app = mount(TfSchemaForm, { target, props: { fields, values, errors, idPrefix: PREFIX, layout } });
  flushSync();
  return { fields, values };
}

function control<T extends HTMLElement>(key: string) {
  return target.querySelector<T>(`#${PREFIX}-${key}`)!;
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("renders a select, a list and a number from the real adapter schemas", () => {
  render(JIRA_SCHEMA);
  const flavor = control<HTMLSelectElement>("flavor");
  expect(flavor.tagName).toBe("SELECT");
  expect([...flavor.options].map((o) => o.value)).toEqual(["datacenter", "cloud"]);
  // The declared default is *selected*, not merely present: a form that opens
  // on the wrong flavor is a form that saves the wrong flavor.
  expect(flavor.value).toBe("datacenter");
  expect(control<HTMLTextAreaElement>("projects").tagName).toBe("TEXTAREA");
  expect(control<HTMLInputElement>("jql_filter").type).toBe("text");

  unmount(app!);
  app = undefined;
  target.replaceChildren();
  render(TEAMCITY_SCHEMA);
  const n = control<HTMLInputElement>("builds_per_config");
  expect(n.type).toBe("number");
  // The bounds are on the element too, so the browser and the validator agree.
  expect(n.min).toBe("1");
  // The adapter's `MAX_BUILDS_PER_CONFIG` (#124).
  expect(n.max).toBe("10000");
  expect(n.value).toBe("100");
});

test("a boolean renders as a checkbox, not as the word true", () => {
  render({ type: "object", properties: { verify_tls: { type: "boolean", default: true } } });
  const box = control<HTMLInputElement>("verify_tls");
  expect(box.type).toBe("checkbox");
  expect(box.checked).toBe(true);
});

test("a list control splits on newlines and keeps what was typed", () => {
  const { fields, values } = render(JIRA_SCHEMA);
  const box = control<HTMLTextAreaElement>("projects");
  box.value = "PAY\n\n  OPS  \n";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  // The textarea keeps what was typed (deleting a blank line under the
  // caret makes a text box unusable); the *validated* config is tidied.
  expect(box.value).toBe("PAY\n\n  OPS  \n");
  expect(validate(fields, { ...values, flavor: "datacenter" }).config).toEqual({
    flavor: "datacenter",
    projects: ["PAY", "OPS"],
  });
});

test("an invalid value shows its message next to its field, and says so accessibly", () => {
  render(TEAMCITY_SCHEMA, {
    builds_per_config: "Builds kept per configuration must be at most 10000.",
  });
  const input = control<HTMLInputElement>("builds_per_config");
  const message = target.querySelector<HTMLElement>(`#${PREFIX}-builds_per_config-err`)!;
  expect(message.textContent).toContain("must be at most 10000");
  expect(input.parentElement!.contains(message)).toBe(true);
  expect(input.getAttribute("aria-invalid")).toBe("true");
  // The message is *named* by the control, so a screen reader reads it with
  // the field rather than leaving it as unattached red text.
  expect((input.getAttribute("aria-describedby") ?? "").split(/\s+/)).toContain(message.id);
});

test("help text is wired to its control, and required is reflected on the element", () => {
  render(JIRA_SCHEMA);
  const flavor = control<HTMLSelectElement>("flavor");
  const help = target.querySelector<HTMLElement>(`#${PREFIX}-flavor-help`)!;
  expect(help.classList.contains("tf-sf-help")).toBe(true);
  expect((flavor.getAttribute("aria-describedby") ?? "").split(/\s+/)).toContain(help.id);
  expect(help.textContent).toContain("Data Center / Server speaks REST v2");
  // Nothing in Jira is required, and the element says so (#124).
  expect(flavor.required).toBe(false);
  expect(control<HTMLInputElement>("jql_filter").required).toBe(false);
});

test("a required property is reflected on its element", () => {
  render({
    type: "object",
    required: ["region"],
    properties: {
      region: { type: "string", enum: ["eu", "us"], title: "Region" },
      note: { type: "string", title: "Note" },
    },
  });
  expect(control<HTMLSelectElement>("region").required).toBe(true);
  expect(control<HTMLInputElement>("note").required).toBe(false);
});

for (const layout of ["grid", "rows"] as const) {
  test(`every control has a label a pointer and a screen reader can both use (${layout})`, () => {
    render(JIRA_SCHEMA, {}, layout);
    for (const key of ["flavor", "projects", "jql_filter", "username"]) {
      const input = control<HTMLElement>(key);
      expect(input.id).not.toBe("");
      const label = target.querySelector<HTMLLabelElement>(`label[for="${input.id}"]`);
      expect(label, key).not.toBeNull();
      expect(label!.textContent!.trim()).not.toBe("");
    }
  });
}

test("an adapter with no configuration shows a sentence, not an empty box", () => {
  render(EMPTY_SCHEMA);
  expect(target.querySelector(".tf-formgrid")).toBeNull();
  expect(target.textContent).toContain("needs no configuration");
});

test("a shape the subset does not cover is editable as JSON rather than lost", () => {
  render({
    type: "object",
    properties: { matrix: { type: "array", items: { type: "object" }, default: [{ a: 1 }] } },
  });
  const box = control<HTMLTextAreaElement>("matrix");
  expect(box.tagName).toBe("TEXTAREA");
  expect(box.value).toContain('"a": 1');
  expect(target.textContent).toContain("JSON");
});

test("two fields never share an element id, so two forms on a page stay wired", () => {
  render(JIRA_SCHEMA);
  const ids = [...target.querySelectorAll("[id]")].map((el) => el.id);
  expect(new Set(ids).size).toBe(ids.length);
});
