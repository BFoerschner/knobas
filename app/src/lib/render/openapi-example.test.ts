/**
 * Examples made from a schema (ux31; Björn: generated "if there's no example data", and "a faker kind of library …
 * fields that have name in it, lastName, first_name"): the order a value is chosen in, the name table in every case
 * style and in German, realistic values by Faker, deterministic, held to the schema's bounds, by direction, with
 * recursion capped, and valid against the schema it was made from.
 */
import { readFileSync } from "node:fs";

import { beforeAll, expect, test } from "vitest";

import { SpecFiles } from "./openapi";
import { DEEP_30_JSON } from "./openapi-fixture";
import { fromPattern } from "./openapi-example";
import { PASSWORD, fakesLoaded, loadFakes, localeOf } from "./openapi-fakes";
import { kindOf, wordsOf } from "./openapi-names";
import { validates } from "./openapi-validate";

function spec(schemas: Record<string, unknown>): SpecFiles {
  const files = new SpecFiles("s.json");
  const answer = files.open(JSON.stringify({ openapi: "3.0.3", info: { title: "E", version: "1" }, paths: {}, components: { schemas } }));
  if (!answer.ok) throw new Error(answer.error);
  return files;
}
const ref = (name: string) => ({ $ref: `#/components/schemas/${name}` });

beforeAll(async () => {
  await loadFakes("en");
});

test("the name table reads every case style, and German names", () => {
  expect(wordsOf("phoneNumber")).toEqual(["phone", "number"]);
  expect(wordsOf("phone_number")).toEqual(["phone", "number"]);
  expect(wordsOf("phone-number")).toEqual(["phone", "number"]);
  expect(wordsOf("HTMLUrl")).toEqual(["html", "url"]);
  const table: [string, string][] = [
    ["firstName", "firstName"], ["first_name", "firstName"], ["first-name", "firstName"], ["givenName", "firstName"], ["vorname", "firstName"], ["Vorname", "firstName"],
    ["lastName", "lastName"], ["surname", "lastName"], ["familyName", "lastName"], ["nachname", "lastName"],
    ["fullName", "fullName"], ["name", "fullName"], ["customer_name", "fullName"],
    ["email", "email"], ["emailAddress", "email"], ["contact_email", "email"],
    ["phone", "phone"], ["phoneNumber", "phone"], ["tel", "phone"], ["mobile", "phone"], ["telefon", "phone"], ["telefonnummer", "phone"], ["mobilePhone", "phone"],
    ["street", "street"], ["address", "street"], ["strasse", "street"], ["straße", "street"],
    ["city", "city"], ["ort", "city"],
    ["zip", "zip"], ["postalCode", "zip"], ["postal_code", "zip"], ["plz", "zip"],
    ["country", "country"], ["land", "country"], ["countryCode", "countryCode"],
    ["company", "company"], ["organization", "company"], ["firma", "company"],
    ["iban", "iban"], ["IBAN", "iban"], ["bic", "bic"],
    ["username", "username"], ["login", "username"],
    ["password", "password"], ["newPassword", "password"], ["passwort", "password"],
    ["url", "url"], ["website", "url"], ["html_url", "url"],
    ["avatar", "avatar"], ["avatar_url", "avatar"], ["image", "avatar"],
    ["price", "price"], ["amount", "price"], ["betrag", "price"],
    ["currency", "currency"],
    ["insuranceNumber", "insuranceNumber"], ["versicherungsnummer", "insuranceNumber"],
    ["taxId", "taxId"], ["steuernummer", "taxId"],
    ["birthDate", "birthDate"], ["date_of_birth", "birthDate"], ["geburtsdatum", "birthDate"],
    ["created_at", "dateTime"], ["updatedAt", "dateTime"], ["lastLogin", "dateTime"],
    ["user_id", "id"], ["orderId", "id"], ["id", "id"],
    ["pageNumber", "count"],
  ];
  expect(table.filter(([name, kind]) => kindOf(name) !== kind).map(([name, kind]) => `${name}: ${kindOf(name)} not ${kind}`)).toEqual([]);
  expect(kindOf("frobnicate")).toBeNull();
  expect(kindOf("")).toBeNull();
});

test("the order: the spec's example or default, then enum or const, then format, then pattern, then the name, then the type", () => {
  const files = spec({
    T: {
      type: "object",
      properties: {
        email: { type: "string", format: "email", example: "given@spec.test", enum: ["a@b.c"] },
        city: { type: "string", default: "Hamburg", enum: ["Berlin"] },
        phone: { type: "string", enum: ["+1 555 0100"], format: "uuid" },
        firstName: { const: "Ada", format: "email" },
        lastName: { const: "Lovelace", default: "Byron" },
        zip: { type: "string", format: "uuid", pattern: "^[0-9]{5}$" },
        plz: { type: "string", pattern: "^[0-9]{5}$" },
        url: { type: "string" },
        frobnicate: { type: "string" },
      },
    },
  });
  const t = files.sample(files.root, ref("T")) as Record<string, string>;
  expect(t.email).toBe("given@spec.test");
  expect(t.city).toBe("Hamburg");
  expect(t.phone).toBe("+1 555 0100");
  expect(t.firstName).toBe("Ada");
  // The spec's default comes before a const.
  expect(t.lastName).toBe("Byron");
  // A format beats a pattern, and a pattern beats the name.
  expect(t.zip).toBe("3fa85f64-5717-4562-b3fc-2c963f66afa6");
  expect(t.plz).toMatch(/^[0-9]{5}$/);
  expect(t.url).toMatch(/^https:\/\/[a-z0-9.-]+\/$/);
  expect(t.frobnicate).toBe("example");
});

test("names make realistic values by Faker; a password or a token is always a placeholder", () => {
  expect(fakesLoaded()).toBe(true);
  const files = spec({
    T: {
      type: "object",
      properties: {
        first_name: { type: "string" },
        email: { type: "string" },
        phoneNumber: { type: "string" },
        iban: { type: "string" },
        password: { type: "string", example: "hunter2", minLength: 12 },
        api_key: { type: "string" },
        amount: { type: "integer", minimum: 1 },
        birthDate: { type: "string", format: "date" },
      },
    },
  });
  const t = files.sample(files.root, ref("T")) as Record<string, unknown>;
  expect(t.first_name).toMatch(/^[A-Z][a-z]+/);
  expect(t.email).toMatch(/^[^@\s]+@example\.com$/);
  expect(t.phoneNumber).toMatch(/^\+\d[\d ]+$/);
  expect(t.iban).toMatch(/^[A-Z]{2}\d{2}[A-Z0-9]+$/);
  expect(t.password).toBe(PASSWORD);
  expect(t.api_key).toBe("<token>");
  expect(Number.isInteger(t.amount) && (t.amount as number) >= 1).toBe(true);
  expect(t.birthDate).toBe("2026-11-22");
});

test("the same schema always gives the same example; another path, another value", () => {
  const files = new SpecFiles("p.json");
  if (!files.open(DEEP_30_JSON).ok) throw new Error("no spec");
  const once = JSON.stringify(files.sample(files.root, ref("Charge"), "response"));
  const again = JSON.stringify(files.sample(files.root, ref("Charge"), "response"));
  expect(again).toBe(once);
  const people = spec({ T: { type: "object", properties: { author: { type: "object", properties: { name: { type: "string" } } }, owner: { type: "object", properties: { name: { type: "string" } } } } } });
  const t = people.sample(people.root, ref("T")) as Record<string, { name: string }>;
  expect(t.author!.name).not.toBe(t.owner!.name);
});

test("numbers and strings keep to their bounds", () => {
  const files = spec({
    T: {
      type: "object",
      properties: {
        low: { type: "integer", minimum: 5 },
        high: { type: "integer", maximum: 0 },
        over: { type: "integer", exclusiveMinimum: 9 },
        step: { type: "integer", minimum: 3, multipleOf: 5 },
        ratio: { type: "number", maximum: 1 },
        short: { type: "string", minLength: 10 },
        eight: { type: "string", minLength: 8 },
        tiny: { type: "string", maxLength: 3 },
      },
    },
  });
  expect(files.sample(files.root, ref("T"))).toEqual({ low: 5, high: 0, over: 10, step: 5, ratio: 1, short: "examplexxx", eight: "examplex", tiny: "exa" });
});

test("a simple pattern is made by hand before randexp loads, and a hard one is left alone", () => {
  expect(fromPattern("^[0-9]{4}$")).toBe("0000");
  expect(fromPattern("^[a-z]+-[a-z]{2,4}$")).toBe("a-aa");
  expect(fromPattern("^\\d{3}-\\d{2}$")).toBe("111-11");
  expect(fromPattern("^(a|b)$")).toBeNull();
  expect(fromPattern("^.+$")).toBeNull();
  // A string made by hand that its own pattern refuses is no answer.
  expect(fromPattern("a^b")).toBeNull();
  const files = spec({ T: { type: "object", properties: { last4: { type: "string", pattern: "^[0-9]{4}$" }, choice: { type: "string", pattern: "^(red|green)-[a-f]{3}$" } } } });
  const t = files.sample(files.root, ref("T")) as Record<string, string>;
  expect(t.last4).toMatch(/^[0-9]{4}$/);
  expect(t.choice).toMatch(/^(red|green)-[a-f]{3}$/);
});

test("a request leaves out read-only fields and an answer write-only ones", () => {
  const files = spec({ T: { type: "object", properties: { id: { type: "integer", readOnly: true }, secret: { type: "string", writeOnly: true }, title: { type: "string" } } } });
  expect(Object.keys(files.sample(files.root, ref("T"), "request") as object)).toEqual(["secret", "title"]);
  expect(Object.keys(files.sample(files.root, ref("T"), "response") as object)).toEqual(["id", "title"]);
  expect(Object.keys(files.sample(files.root, ref("T"), "any") as object)).toEqual(["id", "secret", "title"]);
});

test("allOf merges, oneOf takes the choice asked for, recursion closes", () => {
  const files = new SpecFiles("p.json");
  if (!files.open(DEEP_30_JSON).ok) throw new Error("no spec");
  const charge = files.sample(files.root, ref("Charge"), "response") as Record<string, unknown>;
  expect(Object.keys(charge)).toEqual(["id", "object", "amount", "customer", "description", "source", "parent"]);
  expect(Object.keys(charge.source as object)).toEqual(["id", "name"]);
  expect(charge.parent).toBeNull();
  const card = files.sample(files.root, { oneOf: [ref("Card"), ref("Bank")] }, "response", 0) as Record<string, unknown>;
  const bank = files.sample(files.root, { oneOf: [ref("Card"), ref("Bank")] }, "response", 1) as Record<string, unknown>;
  expect(card).toHaveProperty("last4");
  expect(bank).toHaveProperty("iban");
});

test("an array holds one item, or as many as minItems asks, at most three", () => {
  const files = spec({ T: { type: "object", properties: { tags: { type: "array", items: { type: "string" } }, pair: { type: "array", minItems: 2, items: { type: "integer" } } } } });
  expect(files.sample(files.root, ref("T"))).toEqual({ tags: ["example"], pair: [1, 1] });
});

test("the locale follows the window's language: German for de, else English", () => {
  expect(localeOf("de-DE")).toBe("de");
  expect(localeOf("de")).toBe("de");
  expect(localeOf("en-GB")).toBe("en");
  expect(localeOf(undefined)).toBe("en");
});

/**
 * Every named schema of a real spec, as an answer: the example validates against the schema it was made from. Runs
 * where `KNOBAS_OAS_SPECS` names a folder holding `github.json` and `stripe.json` (14 MB and 8 MB, not in the
 * repository); the ux31 build ran it over both.
 */
const real = process.env["KNOBAS_OAS_SPECS"];
test.skipIf(real === undefined)("every schema of GitHub's and Stripe's specs makes an example that validates, in time", () => {
  for (const name of ["github.json", "stripe.json"]) {
    const files = new SpecFiles(name);
    const opened = files.open(readFileSync(`${real}/${name}`, "utf8"));
    if (!opened.ok) throw new Error(opened.error);
    const failed: string[] = [];
    let worst = 0;
    for (const schema of opened.index.schemas) {
      const t0 = performance.now();
      const value = files.sample(files.root, ref(schema), "response", 0, false);
      worst = Math.max(worst, performance.now() - t0);
      const why = validates(files, files.root, ref(schema), value, "response");
      if (why !== null) failed.push(`${schema}: ${why}`);
    }
    console.log(`${name}: ${opened.index.schemas.length} schemas, ${failed.length} invalid, slowest ${worst.toFixed(1)} ms`);
    if (name === "stripe.json") {
      // Stripe's charges: the whole page answer, its body and answers made, as the worker makes it (section 29).
      const t0 = performance.now();
      const page = files.page("post /v1/charges");
      const ms = performance.now() - t0;
      console.log(`stripe.json post /v1/charges page with its examples: ${ms.toFixed(1)} ms`);
      expect(page.kind).toBe("op");
      expect(ms).toBeLessThan(50);
    }
    expect(failed.slice(0, 10)).toEqual([]);
  }
});
