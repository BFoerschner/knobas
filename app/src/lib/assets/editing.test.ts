/**
 * The Tree's form arithmetic (issue #429).
 *
 * The fixture is a **corner of the real type table**, not the whole of it and
 * not an invented one: spec #427 rules the estate is the real thing, and a
 * table of two types cannot witness "the usual ones first, then the rest, and
 * the two add up to all of them". Four types with three different suggestion
 * shapes — a chain link, a leaf, and the escape hatch — is the smallest
 * fixture that can.
 */
import { expect, test } from "vitest";

import type { AssetProperty, AssetType, PropertyKind } from "../ipc/assets";
import {
  defaultTypeId,
  draftOf,
  inputTypeFor,
  kindFor,
  parseProperty,
  PROPERTY_KINDS,
  propertyEdit,
  typeChoices,
  usualHere,
} from "./editing";

const VM: AssetType = {
  id: "vm",
  label: "VM",
  monogram: "VM",
  properties: [
    { key: "hostname", label: "Hostname", kind: "text" },
    { key: "ip", label: "IP", kind: "text" },
  ],
  suggests: ["container_engine", "service"],
};

const ENGINE: AssetType = {
  id: "container_engine",
  label: "Container engine",
  monogram: "CE",
  properties: [{ key: "version", label: "Version", kind: "text" }],
  suggests: ["container"],
};

const SERVICE: AssetType = {
  id: "service",
  label: "Service",
  monogram: "SV",
  properties: [
    { key: "url", label: "URL", kind: "url" },
    { key: "port", label: "Port", kind: "number" },
  ],
  suggests: [],
};

const CUSTOM: AssetType = {
  id: "custom",
  label: "Custom",
  monogram: "CU",
  properties: [],
  suggests: [],
};

/** The table's own order — spec #427's, outermost thing first. */
const TABLE = [VM, ENGINE, SERVICE, CUSTOM];

function property(over: Partial<AssetProperty> & Pick<AssetProperty, "key">): AssetProperty {
  return { label: over.key, value: null, custom: false, ...over };
}

/**
 * **The usual types come first and the rest are still offered.**
 *
 * Story 17 is *"the type conventions suggesting what usually goes here and
 * **any type allowed**"*, so the assertion is on both halves: what is usual,
 * and that the two lists together are the whole table. A `usual` that filtered
 * would satisfy the first half and break the feature.
 */
test("a parent's conventions order the list without shortening it", () => {
  const choices = typeChoices(TABLE, "vm");

  expect(choices.usual.map((type) => type.id)).toEqual(["container_engine", "service"]);
  expect(choices.rest.map((type) => type.id)).toEqual(["vm", "custom"]);
  expect([...choices.usual, ...choices.rest]).toHaveLength(TABLE.length);
  expect(usualHere(choices)).toBe("Usual here: Container engine, Service");
  expect(defaultTypeId(choices)).toBe("container_engine");
});

/**
 * **The suggestions are drawn in the table's order, not the parent's.**
 *
 * `suggests` is a list of ids and the table is the list of types; reading the
 * first through the second is what keeps one ordering in the app. Asserted
 * with a parent whose `suggests` is in the opposite order to the table's, so a
 * `map` over `suggests` — the obvious other implementation — fails here.
 */
test("the usual types keep the type table's order", () => {
  const reversed: AssetType = {
    ...VM,
    suggests: ["service", "container_engine"],
  };

  expect(typeChoices([reversed, ENGINE, SERVICE, CUSTOM], "vm").usual.map((t) => t.id)).toEqual([
    "container_engine",
    "service",
  ]);
});

/**
 * **The top of the estate has no convention, and says so by drawing none.**
 *
 * There is no parent there to have one. The default is the table's head, which
 * is the outermost type spec #427 lists first — so the dialog opens on the
 * thing that usually goes at the top without anything having to claim it is
 * *usual*.
 */
test("nothing is usual at the top of the estate and everything is offered", () => {
  const choices = typeChoices(TABLE, null);

  expect(choices.usual).toEqual([]);
  expect(choices.rest).toEqual(TABLE);
  expect(usualHere(choices)).toBeNull();
  expect(defaultTypeId(choices)).toBe("vm");
});

/**
 * **A type that suggests nothing, and a type the build has lost, behave the
 * same way: everything is offered.**
 *
 * The second is reachable — an estate file (#439) may name a type this build
 * does not carry, and the Tree draws that asset with a `??` chip rather than
 * refusing it, so a plus on its column has to open on something.
 */
test("a type with no conventions and a type nobody declares both offer the whole table", () => {
  for (const parent of ["custom", "scenario"]) {
    const choices = typeChoices(TABLE, parent);
    expect(choices.usual, parent).toEqual([]);
    expect(choices.rest, parent).toEqual(TABLE);
  }
});

/**
 * **An unfilled typed property still knows what kind it is.**
 *
 * This is the reason the type table is on the wire. `AssetProperty` carries
 * `value: null` for a declared key nobody has filled in, and there is no kind
 * in a `null` — so the editor would be guessing, and `assets::edit` refuses a
 * typed key given the wrong kind by name.
 */
test("the kind of a property comes from its value, then from its type's schema", () => {
  // Filled in: the value says what it is, whatever the schema declares.
  expect(kindFor(property({ key: "port", value: { kind: "number", value: 8080 } }), SERVICE)).toBe(
    "number",
  );
  // Declared and unfilled: only the schema knows.
  expect(kindFor(property({ key: "port" }), SERVICE)).toBe("number");
  expect(kindFor(property({ key: "url" }), SERVICE)).toBe("url");
  // A custom key, and a type this build has lost: text is what a reader can
  // always type.
  expect(kindFor(property({ key: "backup window", custom: true }), SERVICE)).toBe("text");
  expect(kindFor(property({ key: "port" }), undefined)).toBe("text");
});

test("an editor opens on the value that is there, and on nothing when there is none", () => {
  expect(draftOf(property({ key: "ip", value: { kind: "text", value: "10.0.0.4" } }))).toBe(
    "10.0.0.4",
  );
  expect(draftOf(property({ key: "port", value: { kind: "number", value: 8080 } }))).toBe("8080");
  expect(draftOf(property({ key: "ports" }))).toBe("");
});

/**
 * **The four kinds are enumerated once, and every one of them can be typed
 * into and parsed.**
 *
 * The table is what the *add a property* picker draws and what chooses an
 * `<input type>`, so the assertion is that the two agree with the wire's own
 * union — the pane offering a kind the parser does not handle would be a
 * control that produces a value the backend refuses.
 */
test("every property kind is offered, has a control, and parses", () => {
  // The pairs, written out — **not** read back off the table. `inputTypeFor`
  // reads that table, so asserting `entry.input` against it would be the
  // assertion agreeing with itself: swapping `number`'s control for a text box
  // would pass, and a text box is exactly what lets `8080 ` through to a JSON
  // `null`. The date picker is the same claim: it is the one control that
  // cannot produce anything but `YYYY-MM-DD`, which is what the backend wants.
  expect(PROPERTY_KINDS.map((entry) => [entry.kind, entry.input])).toEqual([
    ["text", "text"],
    ["number", "number"],
    ["date", "date"],
    ["url", "url"],
  ]);
  for (const entry of PROPERTY_KINDS) {
    expect(entry.label.trim(), entry.kind).not.toBe("");
    expect(inputTypeFor(entry.kind), entry.kind).toBe(entry.input);
    expect(parseProperty(entry.kind, "1"), entry.kind).not.toHaveProperty("refused");
  }
  // A kind this build has lost still gets a control a reader can type into.
  expect(inputTypeFor("secret" as PropertyKind)).toBe("text");
});

/**
 * **A field emptied is a property cleared, and a cleared property is `null` on
 * the wire.**
 *
 * `PropertyValue::vet` refuses a blank text with *"clear the property
 * instead"*, so blank has to be read as the clear the reader meant rather than
 * sent as a value the backend will reject. `null` on a `PropertyEdit` removes
 * the key — the unambiguous clear the tagged union exists for.
 */
test("a blank field is a clear, whatever kind it is", () => {
  for (const kind of ["text", "number", "date", "url"] as const) {
    expect(parseProperty(kind, "   "), kind).toEqual({ value: null });
  }
  expect(propertyEdit("ip", null)).toEqual({
    field: "property",
    key: "ip",
    value: null,
  });
});

/**
 * **Every kind arrives tagged, and a number arrives as a number.**
 *
 * The number is the one the wire cannot carry wrong: `JSON.stringify(NaN)` is
 * `null`, so an unparsable number sent anyway would arrive as a *cleared*
 * property — a different edit from the one that was made, and one no backend
 * refusal could tell apart from a deliberate clear.
 */
test("each kind is sent tagged, and a number that is not one is refused in place", () => {
  expect(parseProperty("text", "  Debian 13 ")).toEqual({
    value: { kind: "text", value: "Debian 13" },
  });
  expect(parseProperty("number", " 8080 ")).toEqual({
    value: { kind: "number", value: 8080 },
  });
  expect(parseProperty("date", "2026-09-06")).toEqual({
    value: { kind: "date", value: "2026-09-06" },
  });
  // Loopback, because `house-rules.test.ts` refuses any host that could
  // resolve anywhere under `app/src/` — data and comments included.
  expect(parseProperty("url", "http://localhost:3001")).toEqual({
    value: { kind: "url", value: "http://localhost:3001" },
  });

  expect(parseProperty("number", "eight thousand")).toEqual({
    refused: "“eight thousand” is not a number.",
  });
  expect(parseProperty("number", "1e400")).toEqual({
    refused: "“1e400” is not a number.",
  });
});

/**
 * **What the frontend does not refuse, it sends — so the backend's own
 * sentence is what a reader sees.**
 *
 * A date that is not a date and a URL with no scheme are `assets::edit`'s
 * refusals, by name, and duplicating those rules here would be a second copy
 * of a vocabulary that already has one owner. What is asserted is that they
 * are *sent*: a frontend that quietly dropped them would leave the reader with
 * a field that does nothing and no message at all.
 */
test("a value only the backend can judge is sent for it to judge", () => {
  expect(parseProperty("date", "yesterday")).toEqual({
    value: { kind: "date", value: "yesterday" },
  });
  expect(parseProperty("url", "localhost:3001")).toEqual({
    value: { kind: "url", value: "localhost:3001" },
  });
});
