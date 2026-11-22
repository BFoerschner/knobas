/**
 * What a property's name says it holds, for an example made from a schema (ux31; Björn, 2026-11-22: "When there's
 * like fields that have name in it, lastName, first_name etc. or like phonenumber, or insurancenumber"). The name is
 * read in any case style (`firstName`, `first_name`, `first-name`, `FirstName`) and in German (`vorname`, `plz`,
 * `versicherungsnummer`). Pure and small: start-up never holds it, the spec worker does.
 */

/** A kind of value a name asks for. */
export type Kind =
  | "firstName"
  | "lastName"
  | "fullName"
  | "username"
  | "email"
  | "phone"
  | "street"
  | "city"
  | "zip"
  | "state"
  | "country"
  | "countryCode"
  | "company"
  | "iban"
  | "bic"
  | "password"
  | "token"
  | "url"
  | "avatar"
  | "price"
  | "currency"
  | "insuranceNumber"
  | "taxId"
  | "birthDate"
  | "dateTime"
  | "uuid"
  | "sha"
  | "title"
  | "text"
  | "slug"
  | "path"
  | "locale"
  | "colour"
  | "status"
  | "kind"
  | "version"
  | "id"
  | "port"
  | "year"
  | "age"
  | "count"
  | "latitude"
  | "longitude"
  | "jobTitle"
  | "ip";

/** A name's words, lowercase: `phoneNumber`, `phone_number` and `phone-number` are all `["phone", "number"]`. */
export function wordsOf(name: string): string[] {
  return name
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1 $2")
    .split(/[^A-Za-z0-9äöüßÄÖÜ]+/)
    .filter((w) => w !== "")
    .map((w) => w.toLowerCase());
}

/**
 * The table, most particular first: each kind with the joined names that are it (`phonenumber`, `telefon`), and the
 * endings a longer name may have (`mobilePhone` ends `phone`). A name is read joined, its words run together.
 */
const TABLE: readonly (readonly [Kind, readonly string[], readonly string[]])[] = [
  ["password", ["password", "passwort", "passwd", "pwd", "pass", "kennwort"], ["password", "passwort"]],
  ["token", ["token", "secret", "apikey", "accesstoken", "refreshtoken", "clientsecret", "privatekey"], ["token", "secret", "apikey"]],
  ["email", ["email", "mail", "emailaddress", "mailaddress", "emailadresse", "e-mail"], ["email", "mail"]],
  ["firstName", ["firstname", "givenname", "forename", "vorname"], ["firstname", "givenname", "vorname"]],
  ["lastName", ["lastname", "surname", "familyname", "nachname", "familienname"], ["lastname", "surname", "familyname", "nachname"]],
  ["username", ["username", "login", "user", "handle", "nickname", "benutzername", "benutzer", "loginname", "screenname"], ["username", "login", "handle"]],
  ["fullName", ["name", "fullname", "displayname", "contactname", "customername", "personname", "realname", "author", "owner", "assignee"], ["fullname", "displayname", "contactname", "customername"]],
  ["phone", ["phone", "phonenumber", "tel", "telephone", "mobile", "mobilephone", "mobilenumber", "cell", "fax", "telefon", "telefonnummer", "handy", "handynummer", "rufnummer"], ["phone", "phonenumber", "telefon", "telefonnummer", "mobile"]],
  ["street", ["street", "streetaddress", "address", "addressline", "addressline1", "line1", "strasse", "straße", "adresse", "anschrift"], ["street", "strasse", "straße", "streetaddress", "addressline1"]],
  ["city", ["city", "town", "locality", "ort", "stadt", "wohnort"], ["city", "town"]],
  ["zip", ["zip", "zipcode", "postalcode", "postcode", "plz", "postleitzahl"], ["zipcode", "postalcode", "postcode", "plz"]],
  ["state", ["state", "region", "province", "bundesland"], ["province"]],
  ["countryCode", ["countrycode", "countryiso", "landcode", "laendercode"], ["countrycode"]],
  ["country", ["country", "land", "nation"], ["country"]],
  ["company", ["company", "organization", "organisation", "org", "firma", "unternehmen", "companyname", "employer", "arbeitgeber"], ["company", "companyname", "organization", "organisation", "firma"]],
  ["iban", ["iban"], ["iban"]],
  ["bic", ["bic", "swift", "swiftcode", "bankcode"], ["bic"]],
  ["insuranceNumber", ["insurancenumber", "insuranceid", "policynumber", "versicherungsnummer", "sozialversicherungsnummer", "svnr", "ssn", "socialsecuritynumber", "krankenversicherungsnummer"], ["insurancenumber", "versicherungsnummer", "policynumber"]],
  ["taxId", ["taxid", "taxnumber", "vatid", "vatnumber", "steuernummer", "steuerid", "ustid", "ustidnr", "tin"], ["taxid", "taxnumber", "vatid", "steuernummer"]],
  ["birthDate", ["birthdate", "dateofbirth", "dob", "birthday", "geburtsdatum", "geburtstag"], ["birthdate", "dateofbirth", "geburtsdatum"]],
  ["avatar", ["avatar", "avatarurl", "image", "imageurl", "photo", "photourl", "picture", "pictureurl", "bild", "foto", "thumbnail", "icon", "logo"], ["avatarurl", "imageurl", "photourl", "avatar"]],
  ["url", ["url", "uri", "website", "homepage", "href", "link", "webseite", "webpage"], ["url", "uri", "href", "website"]],
  ["price", ["price", "amount", "cost", "total", "subtotal", "fee", "preis", "betrag", "kosten", "summe"], ["price", "amount", "cost", "preis", "betrag"]],
  ["currency", ["currency", "currencycode", "waehrung", "währung"], ["currency"]],
  ["uuid", ["uuid", "guid"], ["uuid", "guid"]],
  ["sha", ["sha", "hash", "commit", "commitsha", "digest", "checksum"], ["sha", "hash", "digest"]],
  ["jobTitle", ["jobtitle", "position", "role", "beruf", "berufsbezeichnung"], ["jobtitle"]],
  ["title", ["title", "summary", "subject", "headline", "label", "titel", "betreff", "caption"], ["title", "summary", "subject"]],
  ["text", ["description", "body", "message", "comment", "text", "note", "notes", "content", "beschreibung", "nachricht", "kommentar", "bio"], ["description", "message", "comment", "beschreibung"]],
  ["slug", ["slug", "key", "code", "ref"], ["slug"]],
  ["path", ["path", "file", "filename", "filepath", "dateiname", "pfad"], ["path", "filename", "filepath"]],
  ["locale", ["locale", "language", "lang", "sprache"], ["locale", "language"]],
  ["colour", ["color", "colour", "farbe"], ["color", "colour"]],
  ["status", ["status", "state"], ["status"]],
  ["kind", ["type", "kind", "category", "typ", "art", "kategorie"], []],
  ["version", ["version", "semver"], ["version"]],
  ["ip", ["ip", "ipaddress", "ipaddr", "remoteaddr", "clientip"], ["ipaddress"]],
  ["latitude", ["lat", "latitude", "breitengrad"], ["latitude"]],
  ["longitude", ["lng", "lon", "long", "longitude", "laengengrad"], ["longitude"]],
  ["port", ["port"], ["port"]],
  ["year", ["year", "jahr"], ["year"]],
  ["age", ["age", "alter"], []],
  ["count", ["count", "total_count", "size", "length", "limit", "perpage", "page", "quantity", "qty", "anzahl", "menge"], ["count"]],
];

/** Names that are a date or a moment by their ending or start, whatever else they say. */
const MOMENT = /(at|on|date|time|timestamp|datum|zeit|zeitpunkt)$/;
const MOMENT_START = /^(created|updated|deleted|modified|started|finished|expires|expired|closed|merged|published|last)/;

/** What `name` asks for, or `null` where the table knows it not. */
export function kindOf(name: string): Kind | null {
  const words = wordsOf(name);
  if (words.length === 0) return null;
  const joined = words.join("");
  for (const [kind, exact] of TABLE) if (exact.includes(joined)) return kind;
  const last = words.at(-1)!;
  // An id is the name's last word: `user_id`, `orderId`. A count, a number: `pageNumber` is not a phone.
  if (last === "id" || last === "ids") return "id";
  if (words.length > 1 && (MOMENT.test(joined) || MOMENT_START.test(joined)) && !/(name|number|nummer)$/.test(joined)) return "dateTime";
  for (const [kind, , endings] of TABLE) for (const end of endings) if (joined.endsWith(end) && joined !== end) return kind;
  if (last === "number" || last === "nummer" || last === "nr") return "count";
  return null;
}
