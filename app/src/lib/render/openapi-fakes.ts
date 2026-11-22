/**
 * Realistic values for an example made from a schema (ux31; Björn, 2026-11-22: "maybe having a faker kind of library
 * to use for addresses, names, phone numbers, etc."): `@faker-js/faker` 10.6.0 (MIT) for a name's {@link Kind}, and
 * `randexp` 0.6.1 (MIT) for a `pattern`. Both are loaded by {@link loadFakes} in the spec worker, lazily, with the
 * first spec, so neither the main bundle nor the first paint carries them; only the locale asked for is loaded
 * (English, or German where the window's language is German).
 *
 * **Deterministic:** each value is made with the generator seeded from the schema path it stands at, so the same
 * schema always shows the same example. Dates are made against a fixed day, never today.
 *
 * Until they are loaded (and in a test that does not load them) {@link fakeValue} answers the fixed table below, which
 * is also deterministic.
 */
import type { Kind } from "./openapi-names";

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
  slug: "example-key",
  path: "docs/readme.md",
  locale: "en",
  colour: "#3366ff",
  status: "active",
  kind: "default",
  version: "1.0.0",
  id: "a1b2c3",
  port: 8080,
  year: 2026,
  age: 34,
  count: 1,
  latitude: 52.52,
  longitude: 13.405,
  jobTitle: "Engineer",
  ip: "192.0.2.10",
};

/** What the loaded libraries answer: a kind's value, and a pattern's string, each from a seed. */
interface Loaded {
  value(kind: Kind, seed: number): string | number;
  pattern(pattern: string, seed: number): string | null;
}

let loaded: Loaded | null = null;
let loading: Promise<void> | null = null;

/** Whether Faker and randexp are loaded. */
export function fakesLoaded(): boolean {
  return loaded !== null;
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

/** Kind `kind`'s value at `path`: Faker's, seeded by the path, once loaded; the fixed table's before. */
export function fakeValue(kind: Kind, path: string): string | number {
  if (kind === "password") return PASSWORD;
  if (kind === "token") return TOKEN;
  if (loaded === null) return FIXED[kind];
  return loaded.value(kind, seedOf(`${kind}\n${path}`));
}

/** A whole number from `min` to `max` at `path`: deterministic, with or without Faker. */
export function fakeInt(path: string, min: number, max: number): number {
  return min + (seedOf(`int\n${path}`) % (max - min + 1));
}

/** A string `pattern` matches, made by randexp seeded by the path once loaded; `null` before, or for a pattern it cannot make. */
export function fakePattern(pattern: string, path: string): string | null {
  return loaded === null ? null : loaded.pattern(pattern, seedOf(`pattern\n${path}`));
}

/** Which Faker locale the window's language asks for. */
export function localeOf(language: string | undefined): "de" | "en" {
  return (language ?? "").toLowerCase().startsWith("de") ? "de" : "en";
}

/** Loads Faker (one locale) and randexp, once. */
export function loadFakes(language: string | undefined): Promise<void> {
  loading ??= (async () => {
    const [{ faker }, RandExp] = await Promise.all([
      localeOf(language) === "de" ? import("@faker-js/faker/locale/de") : import("@faker-js/faker/locale/en"),
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
      title: () => faker.lorem.sentence({ min: 3, max: 6 }).replace(/\.$/, ""),
      text: () => faker.lorem.sentence({ min: 6, max: 12 }),
      slug: () => faker.lorem.slug(2),
      path: () => faker.system.filePath().replace(/^\//, ""),
      locale: () => faker.location.countryCode().toLowerCase(),
      colour: () => faker.color.rgb(),
      status: () => "active",
      kind: () => "default",
      version: () => faker.system.semver(),
      id: () => faker.string.alphanumeric({ length: 8, casing: "lower" }),
      port: () => faker.internet.port(),
      year: () => faker.number.int({ min: 1990, max: 2026 }),
      age: () => faker.number.int({ min: 18, max: 80 }),
      count: () => faker.number.int({ min: 1, max: 50 }),
      latitude: () => faker.location.latitude({ precision: 4 }),
      longitude: () => faker.location.longitude({ precision: 4 }),
      jobTitle: () => faker.person.jobTitle(),
      ip: () => faker.internet.ipv4(),
    };
    loaded = {
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
    };
  })();
  return loading;
}
