/**
 * Examples made from a schema (ux31; Björn: generated "if there's no example data", and "a faker kind of library …
 * fields that have name in it, lastName, first_name"): the order a value is chosen in, the name table in every case
 * style and in German, realistic values by Faker, deterministic, held to the schema's bounds, by direction, with
 * recursion capped, and valid against the schema it was made from.
 */
import { readFileSync, writeFileSync } from "node:fs";

import { beforeAll, expect, test } from "vitest";

import { SpecFiles } from "./openapi";
import { DEEP_30_JSON } from "./openapi-fixture";
import { fromPattern } from "./openapi-example";
import { PASSWORD, fakesLoaded, loadFakes, localeOf } from "./example-fakes";
import { kindOf, phraseOf, wordsOf } from "./example-names";
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
    ["company", "company"], ["companyName", "company"], ["firma", "company"],
    ["iban", "iban"], ["IBAN", "iban"], ["bic", "bic"],
    ["username", "username"], ["login", "username"], ["user", "username"], ["assignee", "username"],
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
    // An API's names (2026-11-22 polish: GitHub's `org` read "example").
    ["org", "org"], ["organization", "org"], ["owner", "org"], ["enterprise", "org"],
    ["repo", "repo"], ["repository", "repo"], ["repo_name", "repo"],
    ["branch", "branch"], ["default_branch", "branch"], ["head_branch", "branch"], ["ref", "branch"], ["target_commitish", "branch"],
    ["tag", "tag"], ["tag_name", "tag"],
    ["sha", "sha"], ["commit", "sha"], ["commit_sha", "sha"], ["head_sha", "sha"], ["commit_id", "sha"],
    ["team_slug", "slug"], ["app_slug", "slug"], ["slug", "slug"], ["key", "slug"], ["secret_name", "slug"], ["runner_group_name", "slug"],
    ["token", "token"], ["access_token", "token"], ["path", "path"], ["file", "path"], ["filename", "path"],
    ["gist_id", "id"], ["repository_id", "id"],
    ["locale", "locale"], ["language", "locale"], ["timezone", "timezone"], ["time_zone", "timezone"], ["tz", "timezone"],
    ["environment", "environment"], ["environment_name", "environment"], ["content_type", "mediaType"],
    ["permission", "permission"], ["visibility", "visibility"], ["privacy", "visibility"], ["severity", "level"],
    ["after", "cursor"], ["before", "cursor"], ["starting_after", "cursor"], ["error", "text"], ["detail", "text"],
    ["topic", "word"], ["q", "word"], ["action", "action"], ["event", "action"], ["user_view_type", "kind"],
    ["description", "text"], ["body", "text"], ["commit_message", "text"], ["release_notes", "text"], ["reason", "text"], ["summary", "title"], ["title", "title"],
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
  const made = files.sample(files.root, ref("T")) as { tags: string[]; pair: number[] };
  expect(made.pair).toEqual([1, 1]);
  // A list of tags holds labels: one real word, not `example` and not a release's `v1.2.3`.
  expect(made.tags).toHaveLength(1);
  expect(made.tags[0]).toMatch(/^[\p{L}]+$/u);
  expect(made.tags[0]).not.toBe("example");
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

/** Words of faker's Latin: none may stand in an example made from a schema. */
const LATIN = /\b(lorem|ipsum|dolor|amet|consectetur|adipisci\w*|tempora|voluptat\w*|quisquam|accusamus|aliquid|explicabo|nesciunt|cupiditate|vero|quia|aut|eius|velit|necessitatibus|sint|ullam|perferendis)\b/i;

test("a free-text field is a plain English phrase from its name and its holder, never Latin", () => {
  expect(phraseOf("description", "full-repository")).toBe("A short description of the repository");
  expect(phraseOf("summary", "Issue")).toBe("A one-line summary of the issue");
  expect(phraseOf("title", "pull-request")).toBe("A title for the pull request");
  expect(phraseOf("commit_message", "")).toBe("A message about the commit");
  expect(phraseOf("comment", "nullable-simple-user")).toBe("A comment on the user");
  expect(phraseOf("notes", "release")).toBe("A few notes on the release");
  expect(phraseOf("description", "")).toBe("A short description");
  expect(phraseOf("body_text", "issue-comment")).toBe("The text of the issue comment");
  expect(phraseOf("description", "PullRequestResponse")).toBe("A short description of the pull request");
  expect(phraseOf("error", "")).toBe("What went wrong");
  const files = spec({
    Repository: { type: "object", properties: { name: { type: "string" }, description: { type: "string" }, owner: ref("SimpleUser"), labels: { type: "array", items: { type: "object", required: ["name", "description"], properties: { name: { type: "string" }, description: { type: "string" } } } } } },
    SimpleUser: { type: "object", properties: { name: { type: "string" }, login: { type: "string" }, bio: { type: "string" } } },
  });
  const made = files.sample(files.root, ref("Repository"), "response") as { name: string; description: string; owner: { name: string; login: string; bio: string }; labels: { name: string; description: string }[] };
  expect(made.description).toBe("A short description of the repository");
  expect(made.owner.bio).toBe("A few words about the user");
  expect(made.labels[0]!.description).toBe("A short description of the label");
  // A bare `name` reads its holder: a repository's name under a repository, a person's under a user, an identifier under a label.
  expect(made.name).toMatch(/^[a-z0-9]+(-[a-z0-9]+)+$/);
  expect(made.owner.name).toMatch(/^\S+ \S+/);
  expect(made.labels[0]!.name).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/);
  expect(made.owner.login).not.toBe("example");
  expect(JSON.stringify(made)).not.toMatch(LATIN);
});

test("a parameter's example is made from its name: org, repo, branch, a 40-hex sha, an id by its type", () => {
  const files = new SpecFiles("s.json");
  const param = (name: string, schema: unknown) => ({ name, in: "path", required: true, schema });
  const answer = files.open(
    JSON.stringify({
      openapi: "3.0.3",
      info: { title: "E", version: "1" },
      paths: {
        "/repos/{owner}/{repo}/x/{org}/{branch}/{sha}/{team_slug}/{gist_id}/{run_id}/{tag}/{timezone}/{token}": {
          get: {
            parameters: [
              param("owner", { type: "string" }), param("repo", { type: "string" }), param("org", { type: "string" }), param("branch", { type: "string" }),
              param("sha", { type: "string" }), param("team_slug", { type: "string" }), param("gist_id", { type: "string" }), param("run_id", { type: "integer" }),
              param("tag", { type: "string" }), param("timezone", { type: "string" }), param("token", { type: "string" }),
            ],
            responses: { "200": { description: "ok" } },
          },
        },
      },
    }),
  );
  if (!answer.ok) throw new Error(answer.error);
  const op = files.operation(answer.index.ops[0]!.i)!;
  const made = files.pageSamples(op);
  const p = (name: string) => made[`p:path:${name}`];
  for (const name of ["owner", "repo", "org", "branch", "sha", "team_slug", "gist_id", "tag", "timezone"]) expect(p(name), name).not.toBe("example");
  expect(p("org")).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/);
  expect(p("repo")).toMatch(/^[a-z0-9]+(-[a-z0-9]+)+$/);
  expect(p("sha")).toMatch(/^[0-9a-f]{40}$/);
  expect(p("gist_id")).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
  expect(typeof p("run_id")).toBe("number");
  expect(p("tag")).toMatch(/^v\d+\.\d+\.\d+$/);
  expect(p("timezone")).toMatch(/^[A-Z][A-Za-z_]+\/[A-Za-z_\/-]+$/);
  expect(p("token")).toBe("<token>");
  // The same page twice reads the same.
  expect(files.pageSamples(op)).toEqual(made);
});

/**
 * Every example made for GitHub's spec, its schemas and every operation's page: no Latin, and how many strings are
 * still `example` (written to `KNOBAS_OAS_REPORT` where set). Runs where `KNOBAS_OAS_SPECS` names a folder holding
 * `github.json`.
 */
test.skipIf(real === undefined)("GitHub's made examples hold no Latin, and few strings are still `example`", () => {
  const files = new SpecFiles("github.json");
  const opened = files.open(readFileSync(`${real}/github.json`, "utf8"));
  if (!opened.ok) throw new Error(opened.error);
  let strings = 0;
  let examples = 0;
  const latin: string[] = [];
  const left = new Map<string, number>();
  const count = (v: unknown, key: string) => {
    if (Array.isArray(v)) for (const x of v) count(x, key);
    else if (v !== null && typeof v === "object") for (const [k, x] of Object.entries(v)) count(x, k);
    else if (typeof v === "string") {
      strings++;
      if (v === "example") {
        examples++;
        left.set(key, (left.get(key) ?? 0) + 1);
      }
      if (LATIN.test(v)) latin.push(`${key}: ${v}`);
    }
  };
  for (const schema of opened.index.schemas) count(files.sample(files.root, ref(schema), "response", 0, false), schema);
  let params = 0;
  let paramExamples = 0;
  for (const row of opened.index.ops) {
    const op = files.operation(row.i);
    if (op === null) continue;
    for (const [key, value] of Object.entries(files.pageSamples(op))) {
      if (key.startsWith("p:")) {
        params++;
        if (value === "example") paramExamples++;
      }
      count(value, key);
    }
  }
  const report = process.env["KNOBAS_OAS_REPORT"];
  if (report !== undefined) {
    const top = [...left].sort((a, b) => b[1] - a[1]).slice(0, 25).map(([k, n]) => `${n} ${k}`);
    writeFileSync(report, `strings ${strings}, still example ${examples}; parameters ${params}, still example ${paramExamples}; Latin ${latin.length}\n${top.join("\n")}\n`);
  }
  expect(latin.slice(0, 10)).toEqual([]);
  expect(paramExamples / params).toBeLessThan(0.15);
});
