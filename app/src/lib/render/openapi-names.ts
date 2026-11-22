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
  | "ip"
  | "org"
  | "repo"
  | "branch"
  | "tag"
  | "timezone"
  | "environment"
  | "mediaType"
  | "permission"
  | "visibility"
  | "level"
  | "word"
  | "action"
  | "cursor";

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
  // An API's names (2026-11-22 polish): an account's login, an organisation's, a repository's, a branch, a tag.
  ["username", ["username", "login", "user", "handle", "nickname", "benutzername", "benutzer", "loginname", "screenname", "assignee", "actor", "sender", "committer", "reviewer"], ["username", "login", "handle"]],
  ["org", ["org", "organization", "organisation", "owner", "orgname", "organizationname", "orglogin", "enterprise", "workspace", "namespace", "tenant"], ["orgname", "organizationname"]],
  ["repo", ["repo", "repository", "reponame", "repositoryname", "project", "projectname", "projekt"], ["reponame", "repositoryname"]],
  ["branch", ["branch", "branchname", "defaultbranch", "headbranch", "basebranch", "masterbranch", "targetbranch", "sourcebranch", "ref", "headref", "baseref", "targetcommitish", "zweig"], ["branch", "branchname"]],
  ["tag", ["tag", "tagname", "releasetag", "versiontag"], ["tagname"]],
  ["fullName", ["name", "fullname", "displayname", "contactname", "customername", "personname", "realname", "author"], ["fullname", "displayname", "contactname", "customername"]],
  ["phone", ["phone", "phonenumber", "tel", "telephone", "mobile", "mobilephone", "mobilenumber", "cell", "fax", "telefon", "telefonnummer", "handy", "handynummer", "rufnummer"], ["phone", "phonenumber", "telefon", "telefonnummer", "mobile"]],
  ["street", ["street", "streetaddress", "address", "addressline", "addressline1", "line1", "strasse", "straße", "adresse", "anschrift"], ["street", "strasse", "straße", "streetaddress", "addressline1"]],
  ["city", ["city", "town", "locality", "ort", "stadt", "wohnort"], ["city", "town"]],
  ["zip", ["zip", "zipcode", "postalcode", "postcode", "plz", "postleitzahl"], ["zipcode", "postalcode", "postcode", "plz"]],
  ["state", ["state", "region", "province", "bundesland"], ["province"]],
  ["countryCode", ["countrycode", "countryiso", "landcode", "laendercode"], ["countrycode"]],
  ["country", ["country", "land", "nation"], ["country"]],
  ["company", ["company", "firma", "unternehmen", "companyname", "employer", "arbeitgeber"], ["company", "companyname", "firma"]],
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
  ["sha", ["sha", "hash", "commit", "commitsha", "commitid", "commitoid", "oid", "treeid", "digest", "checksum", "revision"], ["sha", "hash", "digest", "commitid", "oid"]],
  ["jobTitle", ["jobtitle", "position", "role", "beruf", "berufsbezeichnung"], ["jobtitle"]],
  ["title", ["title", "summary", "subject", "headline", "label", "titel", "betreff", "caption"], ["title", "summary", "subject"]],
  ["text", ["description", "desc", "body", "message", "comment", "text", "note", "notes", "content", "beschreibung", "nachricht", "kommentar", "bio", "reason", "rationale", "explanation", "detail", "details", "error", "errors", "fehler", "remark", "remarks", "bodytext", "bodyhtml", "notiz", "begruendung"], ["description", "message", "comment", "beschreibung", "notes", "reason", "bodytext", "bodyhtml"]],
  ["slug", ["slug", "key", "code", "context", "identifier"], ["slug", "key"]],
  ["timezone", ["timezone", "tz", "tzid", "zoneid", "zeitzone", "iana"], ["timezone"]],
  ["environment", ["environment", "env", "environmentname", "stage", "umgebung"], ["environment", "environmentname"]],
  ["mediaType", ["contenttype", "mediatype", "mimetype", "mime", "accept"], ["contenttype", "mediatype", "mimetype"]],
  ["permission", ["permission", "accesslevel", "access", "berechtigung"], ["permission", "accesslevel"]],
  ["visibility", ["visibility", "privacy", "privacylevel", "sichtbarkeit"], ["visibility", "privacy"]],
  ["level", ["severity", "priority", "level", "impact", "urgency", "prioritaet", "schweregrad"], ["severity", "priority"]],
  ["word", ["topic", "topics", "keyword", "keywords", "hashtag", "q", "query", "search", "searchterm", "term", "suche", "stichwort"], ["topic", "keyword", "query"]],
  ["action", ["action", "event", "eventtype", "eventname", "ereignis"], ["event"]],
  ["cursor", ["cursor", "after", "before", "startingafter", "endingbefore", "nextcursor", "prevcursor", "pagecursor", "continuation"], ["cursor"]],
  ["path", ["path", "file", "filename", "filepath", "dateiname", "pfad"], ["path", "filename", "filepath"]],
  ["locale", ["locale", "language", "lang", "sprache"], ["locale", "language"]],
  ["colour", ["color", "colour", "farbe"], ["color", "colour"]],
  ["status", ["status", "state"], ["status"]],
  ["kind", ["type", "kind", "category", "typ", "art", "kategorie"], ["type", "kind"]],
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
  // Any other name of a thing (`secret_name`, `runner_group_name`): an identifier, never a person's name.
  if (words.length > 1 && last === "name") return "slug";
  return null;
}

/**
 * The words a schema's or a property's name says it is, for a phrase: `nullable-simple-user` is `user`,
 * `PullRequestResponse` is `pull request`. Words that only qualify go from its start and its end, never its last.
 */
const LEAD = new Set(["nullable", "simple", "full", "minimal", "basic", "base", "public", "new"]);
const TAIL = new Set(["response", "dto", "model", "schema", "object", "item", "items", "data", "info", "payload", "body", "v1", "v2", "v3"]);
export function nounOf(name: string): string {
  const words = wordsOf(name).filter((w) => !/^\d+$/.test(w));
  while (words.length > 1 && LEAD.has(words[0]!)) words.shift();
  while (words.length > 1 && TAIL.has(words.at(-1)!)) words.pop();
  return words.length === 1 && (LEAD.has(words[0]!) || TAIL.has(words[0]!)) ? "" : words.join(" ");
}

/**
 * A short plain English phrase for a free-text field (2026-11-22 polish; Björn's shot: faker's Latin read as
 * noise): from the field's name and the thing that holds it, `parent` (a schema's or a property's name), as
 * `A short description of the repository`. Words in the name before its last say what it is about
 * (`commit_message`: *A message about the commit*). Deterministic, and never Latin.
 */
export function phraseOf(name: string, parent: string): string {
  const words = wordsOf(name);
  const key = words.at(-1) ?? "";
  const before = words.slice(0, -1).filter((w) => !["short", "long", "full", "raw", "plain", "html", "text", "body", "md", "markdown", "display"].includes(w));
  const thing = before.length > 0 ? before.join(" ") : nounOf(parent);
  const of = (lead: string, join: string) => (thing === "" ? lead : `${lead} ${join} the ${thing}`);
  switch (key) {
    case "description":
    case "desc":
    case "beschreibung":
      return of("A short description", "of");
    case "summary":
      return of("A one-line summary", "of");
    case "title":
    case "headline":
    case "titel":
    case "caption":
      return of("A title", "for");
    case "subject":
    case "betreff":
      return of("The subject", "of");
    case "label":
      return of("A label", "for");
    case "message":
    case "nachricht":
      return of("A message", "about");
    case "comment":
    case "kommentar":
    case "remark":
    case "remarks":
      return of("A comment", "on");
    case "note":
    case "notes":
    case "notiz":
      return of("A few notes", "on");
    case "bio":
      return of("A few words", "about");
    case "reason":
    case "rationale":
    case "explanation":
    case "begruendung":
      return of("The reason", "for");
    case "body":
    case "content":
    case "text":
    case "html":
    case "detail":
    case "details":
      return of("The text", "of");
    case "error":
    case "errors":
    case "fehler":
      return of("What went wrong", "with");
    default:
      return thing === "" ? `Some ${words.join(" ")}` : `The ${words.join(" ")} of the ${thing}`;
  }
}
