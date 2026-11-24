// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).

/**
 * What came back: an answer's head (its body is read apart, `api_answer_body`), or where it failed.
 */
export type ApiAnswer = { "kind": "answered", id: number, status: number, reason: string, version: string, headers: Array<[string, string]>, 
/**
 * The body's size as kept.
 */
bytes: number, wire_bytes: number, to_headers_ms: number, total_ms: number, hops: Array<Hop>, 
/**
 * The body was longer than 64 MiB and was cut there.
 */
cut: boolean, 
/**
 * A redirect not followed (another origin, or not a `GET` or `HEAD`): where it points.
 */
follow: string | null, content_type: string, } | { "kind": "failed", id: number, 
/**
 * `resolve`, `connect`, `tls`, `send`, `read`, `timeout` or `stopped`.
 */
stage: string, words: string, };

/**
 * Which held secret a request uses: the origin it was typed for, its scheme's
 * name, and where it goes. Never the secret itself.
 */
export type ApiAuth = { origin: string, scheme: string, place: AuthPlace, 
/**
 * The header's, the query parameter's or the cookie's name, for an API key.
 */
name: string, };

/**
 * A request as the page hands it over: what `outgoing` built, with no secret in it.
 */
export type ApiSend = { 
/**
 * The page's number for this send, which *Stop* and the body's read name.
 */
id: number, method: string, url: string, headers: Array<[string, string]>, body: string | null, auth: ApiAuth | null, 
/**
 * The whole send's limit in seconds, 5 to 600; 60 where left out.
 */
timeout_s: number | null, 
/**
 * The host the page's question was answered *yes* for, for a request that changes something.
 */
confirmed_host: string | null, 
/**
 * A saved server whose CA file to trust, for a request to its origin.
 */
server: string | null, 
/**
 * The spec's file (`repository\npath`) and the operation, for *Sent since knobas started*.
 */
file: string, operation: string, };

/**
 * Where a held secret goes in a request.
 */
export type AuthPlace = "bearer" | "basic" | "header" | "query" | "cookie";

/**
 * Which shipped API clients are installed.
 */
export type ClientsFound = { bruno: boolean, };

/**
 * An image fetched: what it is, how big, and its bytes.
 */
export type FetchedImage = { url: string, mime: string, size: number, base64: string, };

/**
 * One redirect followed.
 */
export type Hop = { status: number, url: string, };

/**
 * The hosts kept: their images load when their dialog opens.
 */
export type ImageHosts = { hosts: Array<string>, };

/**
 * What an open did: the argument vector, the folder, and what was left out.
 */
export type OpenedSpec = { argv: Array<string>, folder: string, 
/**
 * Whether the folder was written now; `false` where it was opened as it was.
 */
wrote: boolean, left_out: Array<[string, string]>, };

/**
 * One reference answered by hand: the namespace and location a schema or a
 * document asked for, and what answers them.
 */
export type PointedSchema = { namespace: string, location: string, to: PointedTo, };

/**
 * Where a pointed reference is answered from: a file of a synced repository,
 * or an address knobas downloaded on a press.
 */
export type PointedTo = { repo: string, path: string, name?: string, } | { url: string, sha256: string, at: string, };

/**
 * One saved server.
 */
export type SavedServer = { 
/**
 * Kept across saves; made from the name for a new one.
 */
id: string, name: string, 
/**
 * `http` or `https`, a host, maybe a port and a path; no user, query or fragment.
 */
url: string, 
/**
 * A PEM file of roots to trust for this server only.
 */
ca_file: string | null, };

/**
 * The saved list, and the server last used with each spec file.
 */
export type SavedServers = { servers: Array<SavedServer>, 
/**
 * `repository\npath` to a server's id.
 */
last: { [key in string]: string }, };

/**
 * Everything pointed to.
 */
export type SchemaLocations = { pointed: Array<PointedSchema>, };

/**
 * One line of *Sent since knobas started*.
 */
export type SentLine = { id: number, at_ms: number, send: ApiSend, answer: ApiAnswer, 
/**
 * The body is still kept and can be drawn again.
 */
kept: boolean, };

/**
 * What the page hands over: the spec as shown and, where asked, its collection.
 */
export type SpecToOpen = { 
/**
 * The API's title.
 */
name: string, spec_text: string, 
/**
 * `yaml` or `json`.
 */
spec_ext: string, 
/**
 * The collection's files, by path from its folder; `None` where the command does not name `{collection}`.
 */
collection: Array<[string, string]> | null, 
/**
 * `repository\npath`: which folder is this spec's.
 */
key: string, };

/**
 * Whether an XML file is checked when a schema is found.
 */
export type XmlCheck = { on: boolean, };
