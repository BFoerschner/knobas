/**
 * Realistic values for an example made from a schema (ux31; Björn, 2026-11-22: "maybe having a faker kind of library
 * to use for addresses, names, phone numbers, etc."): `@faker-js/faker` 10.6.0 (MIT) for a name's {@link Kind}, and
 * `randexp` 0.6.1 (MIT) for a `pattern`. Both are loaded by {@link loadFakes} in the spec worker, lazily, with the
 * first spec, so neither the main bundle nor the first paint carries them; only the locale asked for is loaded
 * (English, or German where the window's language is German). The schema explorer's examples ask by the language
 * picked, not the window's (the XSD design, section 20): each locale asked for is loaded once and kept, so both are
 * held once both were asked for; a value asked without a language is the first locale's, as the API explorer asks.
 *
 * **Deterministic:** each value is made with the generator seeded from the schema path it stands at, so the same
 * schema always shows the same example. Dates are made against a fixed day, never today.
 *
 * Until they are loaded (and in a test that does not load them) {@link fakeValue} answers the fixed table below, which
 * is also deterministic.
 */
import type { Kind } from "./example-names";

/** The day dates are made against, so an example never changes with the calendar. */
/** The scheme of an example's web address. */
const WEB = "https:";

export const REF_DATE = "2026-11-22T14:05:00Z";

/** A password field's example: always a placeholder, never a real-looking secret. */
export const PASSWORD = "<password>";
/** A token's or a key's example: a placeholder, too. */
export const TOKEN = "<token>";

/** The values without Faker: fixed, one a kind. */
const FIXED: Readonly<Record<Kind, string | number>> = {
  firstName: "Mara",
  lastName: "Lindqvist",
  fullName: "Mara Lindqvist",
  username: "mara",
  email: "mara@example.com",
  phone: "+49 30 1234567",
  street: "Hauptstraße 12",
  city: "Berlin",
  zip: "10115",
  state: "Berlin",
  country: "Germany",
  countryCode: "DE",
  company: "Tidewater GmbH",
  iban: "DE89370400440532013000",
  bic: "COBADEFFXXX",
  password: PASSWORD,
  token: TOKEN,
  url: "https://example.com/",
  avatar: "https://example.com/avatar.png",
  price: 19.99,
  currency: "EUR",
  insuranceNumber: "12 150385 L 123",
  taxId: "12/345/67890",
  birthDate: "1985-03-15",
  dateTime: REF_DATE,
  uuid: "3fa85f64-5717-4562-b3fc-2c963f66afa6",
  sha: "0f3a9c1d2e4b5a6978869504a3b2c1d0e9f8a7b6",
  title: "A short title",
  text: "Some words about it.",
  slug: "steady-harbor",
  path: "docs/readme.md",
  locale: "en-US",
  colour: "#3366ff",
  status: "active",
  kind: "default",
  version: "1.0.0",
  id: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  port: 8080,
  year: 2026,
  age: 34,
  count: 1,
  latitude: 52.52,
  longitude: 13.405,
  jobTitle: "Engineer",
  ip: "192.0.2.10",
  org: "tidewater",
  repo: "payout-service",
  branch: "main",
  tag: "v1.4.2",
  timezone: "Europe/Berlin",
  environment: "production",
  mediaType: "application/json",
  permission: "read",
  visibility: "private",
  level: "high",
  word: "harbor",
  action: "created",
  cursor: "Y3Vyc29yOnYyOpK0",
};

/** A name made of words for a URL: lower case, a hyphen between words, nothing else. */
const slugOf = (text: string) =>
  text
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");

/** What the loaded libraries answer: a kind's value, and a pattern's string, each from a seed. */
interface Loaded {
  value(kind: Kind, seed: number): string | number;
  pattern(pattern: string, seed: number): string | null;
}

/** Each locale loaded, and the first: what a value asked without a language is made in. */
const loadedBy = new Map<"de" | "en", Loaded>();
let first: "de" | "en" | null = null;
const loading = new Map<"de" | "en", Promise<void>>();

/** Whether Faker and randexp are loaded: any locale, or the one named. */
export function fakesLoaded(lang?: "de" | "en"): boolean {
  return lang === undefined ? first !== null : loadedBy.has(lang);
}

/** FNV-1a over `text`: the seed of a schema path. */
export function seedOf(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Kind `kind`'s value at `path`: Faker's in `lang` (else the first locale loaded), seeded by the path; the fixed table's before. */
export function fakeValue(kind: Kind, path: string, lang?: "de" | "en"): string | number {
  if (kind === "password") return PASSWORD;
  if (kind === "token") return TOKEN;
  const loaded = lang !== undefined ? (loadedBy.get(lang) ?? null) : first === null ? null : (loadedBy.get(first) ?? null);
  if (loaded === null) return FIXED[kind];
  return loaded.value(kind, seedOf(`${kind}\n${path}`));
}

/** A whole number from `min` to `max` at `path`: deterministic, with or without Faker. */
export function fakeInt(path: string, min: number, max: number): number {
  return min + (seedOf(`int\n${path}`) % (max - min + 1));
}

/** A string `pattern` matches, made by randexp seeded by the path once loaded; `null` before, or for a pattern it cannot make. */
export function fakePattern(pattern: string, path: string): string | null {
  const loaded = first === null ? null : (loadedBy.get(first) ?? null);
  return loaded === null ? null : loaded.pattern(pattern, seedOf(`pattern\n${path}`));
}

/** Which Faker locale the window's language asks for. */
export function localeOf(language: string | undefined): "de" | "en" {
  return (language ?? "").toLowerCase().startsWith("de") ? "de" : "en";
}

/** Loads Faker in the locale `language` asks for, and randexp, once a locale; a locale loaded is kept. */
export function loadFakes(language: string | undefined): Promise<void> {
  const lang = localeOf(language);
  let held = loading.get(lang);
  if (held === undefined) loading.set(lang, (held = load(lang)));
  return held;
}

async function load(lang: "de" | "en"): Promise<void> {
  const [{ faker }, RandExp] = await Promise.all([
    lang === "de" ? import("@faker-js/faker/locale/de") : import("@faker-js/faker/locale/en"),
    import("randexp").then((m) => m.default),
  ]);
  const ref = new Date(REF_DATE);
  faker.setDefaultRefDate(ref);
  const at = (seed: number) => faker.seed(seed);
  const makers: Record<Kind, () => string | number> = {
    firstName: () => faker.person.firstName(),
    lastName: () => faker.person.lastName(),
    fullName: () => faker.person.fullName(),
    username: () => faker.internet.username().toLowerCase(),
    email: () => faker.internet.email({ provider: "example.com" }).toLowerCase(),
    phone: () => faker.phone.number({ style: "international" }),
    street: () => faker.location.streetAddress(),
    city: () => faker.location.city(),
    zip: () => faker.location.zipCode(),
    state: () => faker.location.state(),
    country: () => faker.location.country(),
    countryCode: () => faker.location.countryCode(),
    company: () => faker.company.name(),
    iban: () => faker.finance.iban({ formatted: false }),
    bic: () => faker.finance.bic(),
    password: () => PASSWORD,
    token: () => TOKEN,
    // An example's address names a made-up host and is never fetched; the scheme stands apart so the source names no host.
    url: () => `${WEB}//${faker.internet.domainName()}/`,
    avatar: () => `${WEB}//${faker.internet.domainName()}/avatar.png`,
    price: () => Number(faker.commerce.price({ min: 5, max: 500 })),
    currency: () => faker.finance.currencyCode(),
    insuranceNumber: () => faker.helpers.replaceSymbols("## ###### ? ###"),
    taxId: () => faker.helpers.replaceSymbols("##/###/#####"),
    birthDate: () => faker.date.birthdate({ mode: "age", min: 18, max: 80, refDate: ref }).toISOString().slice(0, 10),
    dateTime: () => faker.date.recent({ days: 30, refDate: ref }).toISOString().replace(/\.\d{3}Z$/, "Z"),
    uuid: () => faker.string.uuid(),
    sha: () => faker.git.commitSha(),
    // A free-text field is a phrase made from its name (`example-names.ts` `phraseOf`), never Faker's Latin.
    title: () => FIXED.title,
    text: () => FIXED.text,
    slug: () => slugOf(`${faker.word.adjective()} ${faker.word.noun()}`),
    path: () => faker.system.filePath().replace(/^\//, ""),
    locale: () => faker.helpers.arrayElement(["en-US", "en-GB", "de-DE", "fr-FR", "es-ES", "ja-JP", "pt-BR"]),
    colour: () => faker.color.rgb(),
    status: () => "active",
    kind: () => "default",
    version: () => faker.system.semver(),
    // A string id is a UUID; a number id is made by the generator from the schema's type.
    id: () => faker.string.uuid(),
    port: () => faker.internet.port(),
    year: () => faker.number.int({ min: 1990, max: 2026 }),
    age: () => faker.number.int({ min: 18, max: 80 }),
    count: () => faker.number.int({ min: 1, max: 50 }),
    latitude: () => faker.location.latitude({ precision: 4 }),
    longitude: () => faker.location.longitude({ precision: 4 }),
    jobTitle: () => faker.person.jobTitle(),
    ip: () => faker.internet.ipv4(),
    org: () => slugOf(faker.company.name()).split("-").slice(0, 2).join("-"),
    repo: () => slugOf(`${faker.hacker.noun()} ${faker.helpers.arrayElement(["service", "api", "app", "cli", "web", "tools", "sdk"])}`),
    branch: () => `feature/${slugOf(`${faker.hacker.verb()} ${faker.hacker.noun()}`)}`,
    tag: () => `v${faker.system.semver()}`,
    timezone: () => faker.location.timeZone(),
    environment: () => faker.helpers.arrayElement(["production", "staging", "development"]),
    mediaType: () => faker.helpers.arrayElement(["application/json", "text/plain", "text/csv"]),
    permission: () => faker.helpers.arrayElement(["read", "write", "admin"]),
    visibility: () => faker.helpers.arrayElement(["public", "private", "internal"]),
    level: () => faker.helpers.arrayElement(["low", "medium", "high", "critical"]),
    word: () => faker.word.noun(),
    action: () => faker.helpers.arrayElement(["created", "updated", "deleted", "opened", "closed"]),
    // A page's cursor is opaque: letters and digits, as a server hands them out.
    cursor: () => faker.string.alphanumeric({ length: 16 }),
  };
  loadedBy.set(lang, {
    value(kind, seed) {
      at(seed);
      return makers[kind]();
    },
    pattern(pattern, seed) {
      // Only a bounded pattern: no back-reference or lookaround, which randexp cannot honour.
      if (/\\[1-9]|\(\?[=!<]/.test(pattern)) return null;
      let state = seed || 1;
      const r = new RandExp(pattern);
      r.max = 6;
      // A seeded draw (mulberry32), so the same pattern at the same path makes the same string.
      r.randInt = (a: number, b: number) => {
        state = (state + 0x6d2b79f5) | 0;
        let t = state;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        const f = ((t ^ (t >>> 14)) >>> 0) / 4294967296;
        return a + Math.floor(f * (b - a + 1));
      };
      let out: string;
      try {
        out = r.gen();
      } catch {
        return null;
      }
      try {
        return new RegExp(pattern).test(out) ? out : null;
      } catch {
        return null;
      }
    },
  });
  first ??= lang;
}
