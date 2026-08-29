/**
 * The generated form: one control per `SchemaField`, wired so the browser, the
 * assistive layer and the validator all agree about the same field.
 *
 * `schema-form.test.ts` pins the *model*. What is here only exists once there
 * is a DOM: that an enum renders as a select and not as a text box, that a
 * list round-trips through a textarea without losing entries, that a message
 * lands next to the field it is about, and that an adapter which needs no
 * configuration says so instead of showing an empty box.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import SchemaForm from "./SchemaForm.svelte";
import { EMPTY_SCHEMA, JIRA_SCHEMA, TEAMCITY_SCHEMA } from "./fixtures";
import { defaultValues, schemaFields, validate } from "./schema-form";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(schema: unknown, errors: Record<string, string> = {}) {
  const fields = schemaFields(schema);
  const state = $state({ values: defaultValues(fields) });
  app = mount(SchemaForm, {
    target,
    get props() {
      return {
        fields,
        values: state.values,
        errors,
        onchange: (key: string, value: unknown) => {
          state.values = { ...state.values, [key]: value };
        },
      };
    },
  });
  flushSync();
  return { fields, state };
}

function field(key: string) {
  return target.querySelector<HTMLElement>(`[data-field="${key}"]`);
}

function control<T extends HTMLElement>(key: string) {
  return field(key)!.querySelector<T>("input, select, textarea")!;
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
  expect(flavor.value).toBe("datacenter");
  // The declared default is *selected*, not merely present: a form that opens
  // on the wrong flavor is a form that saves the wrong flavor.
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
  // The adapter's `MAX_BUILDS_PER_CONFIG`. This read "500" while the source
  // allowed 10 000, so the browser refused a build budget the source would
  // have taken (#124).
  expect(n.max).toBe("10000");
  expect(n.value).toBe("100");
});

test("a boolean renders as a checkbox, not as the word true", () => {
  render({ type: "object", properties: { verify_tls: { type: "boolean", default: true } } });
  const box = control<HTMLInputElement>("verify_tls");
  expect(box.type).toBe("checkbox");
  expect(box.checked).toBe(true);
});

test("a list control splits on newlines and drops blank lines", () => {
  const { fields, state } = render(JIRA_SCHEMA);
  const box = control<HTMLTextAreaElement>("projects");
  box.value = "PAY\n\n  OPS  \n";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  // The textarea keeps what was typed — deleting a reader's blank line under
  // their caret is how a text box becomes unusable — and the *validated*
  // config is what has been tidied.
  expect(box.value).toBe("PAY\n\n  OPS  \n");
  expect(validate(fields, { ...state.values, flavor: "datacenter" }).config).toEqual({
    flavor: "datacenter",
    projects: ["PAY", "OPS"],
  });
});

test("an invalid value shows its message next to its field, and says so accessibly", () => {
  // The message is handed in as a prop: what is pinned here is that a message
  // lands next to its own field and is named by the control, not what any
  // particular bound is. It is spelled with the adapter's real maximum all the
  // same, so a reader of this file is not taught a bound that does not exist.
  render(TEAMCITY_SCHEMA, {
    builds_per_config: "Builds kept per configuration must be at most 10000.",
  });
  const row = field("builds_per_config")!;
  expect(row.textContent).toContain("must be at most 10000");

  const input = control<HTMLInputElement>("builds_per_config");
  expect(input.getAttribute("aria-invalid")).toBe("true");
  // The message is *named* by the control, so a screen reader reads it with
  // the field rather than leaving it as unattached red text.
  const described = (input.getAttribute("aria-describedby") ?? "").split(/\s+/);
  const messageId = row.querySelector(".err-msg")!.id;
  expect(described).toContain(messageId);
});

test("help text is wired to its control, and required is reflected on the element", () => {
  render(JIRA_SCHEMA);
  const flavor = control<HTMLSelectElement>("flavor");
  const helpId = field("flavor")!.querySelector(".help")!.id;
  expect((flavor.getAttribute("aria-describedby") ?? "").split(/\s+/)).toContain(helpId);
  expect(field("flavor")!.querySelector(".help")!.textContent).toContain(
    "Data Center / Server speaks REST v2",
  );
  // Nothing in Jira is required, and the element says so: the fixture used to
  // invent `required: ["flavor"]`, which drew a `<select>` the browser would
  // block a submit on where the real form draws an optional one (#124).
  expect(flavor.required).toBe(false);
  expect(control<HTMLInputElement>("jql_filter").required).toBe(false);
});

/**
 * `required` reaches the element from the schema that declares it.
 *
 * Driven through a schema written here rather than through an adapter,
 * because no shipped adapter declares `required` — and the fixture that
 * pretended one did is what this file used to prove the rule against (#124).
 * The property still matters: without it the browser and the assistive layer
 * are told a different story from `validate`.
 */
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

test("every control has a label a pointer and a screen reader can both use", () => {
  render(JIRA_SCHEMA);
  for (const key of ["flavor", "projects", "jql_filter", "username"]) {
    const input = control<HTMLElement>(key);
    // The label is the grid's left cell, a sibling of the control's cell —
    // `.form` is a two-column grid and a label wrapping its control would
    // break the alignment the whole form is built on.
    const label = target.querySelector<HTMLLabelElement>(`label[data-field-label="${key}"]`)!;
    expect(label.getAttribute("for")).toBe(input.id);
    expect(input.id).not.toBe("");
  }
});

test("an adapter with no configuration shows a sentence, not an empty box", () => {
  render(EMPTY_SCHEMA);
  expect(target.querySelector(".form")).toBeNull();
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
  expect(field("matrix")!.textContent).toContain("JSON");
});

test("two fields never share an element id, so two forms on a page stay wired", () => {
  render(JIRA_SCHEMA);
  const ids = [...target.querySelectorAll("[id]")].map((el) => el.id);
  expect(new Set(ids).size).toBe(ids.length);
});
