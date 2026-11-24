/**
 * The API explorer's way out — `crates/knobas-app/src/commands/api.rs` (ux31, ADR-0057). The types are generated from
 * the Rust (`./generated/api.ts`); the `invoke` functions stay hand-written. Nothing here ever answers a secret: a
 * secret typed on the page is handed to the app once (`apiHoldSecret`) and the page keeps none.
 */
import { invoke } from "./invoke";
import type { PipeRun, PipeStep } from "./entity";
import type { ApiAnswer, ApiAuth, ApiSend, AuthPlace, ClientsFound, DeclaringFile, DownloadedSchema, FetchedImage, JsonQueryAnswer, JsonPathStep, Hop, ImageHosts, OpenedSpec, PointedSchema, PointedTo, SavedServer, SavedServers, SchemaLocations, SentLine, SpecToOpen, XmlCheck } from "./generated/api";

export type { ApiAnswer, ApiAuth, ApiSend, AuthPlace, ClientsFound, FetchedImage, Hop, ImageHosts, OpenedSpec, PointedSchema, PointedTo, SavedServer, SavedServers, SchemaLocations, DeclaringFile, DownloadedSchema, JsonQueryAnswer, JsonPathStep, SentLine, SpecToOpen, XmlCheck };

/** Sends one request, once. A refusal before anything was sent rejects; a failure is an answer. */
export function apiSend(send: ApiSend): Promise<ApiAnswer> {
  return invoke<ApiAnswer>("api_send", { send });
}

/** Stops send `id` at once. */
export function apiCancel(id: number): Promise<void> {
  return invoke<void>("api_cancel", { id });
}

/** What `api_answer_body` answers: an `ArrayBuffer` from the bridge, bytes from the dev fake. */
type RawBytes = ArrayBuffer | ArrayLike<number>;

/** The body of send `id`, JSON indented where `pretty`; rejects once it is no longer kept. */
export async function apiAnswerBody(id: number, pretty: boolean): Promise<Uint8Array> {
  const answer = await invoke<RawBytes>("api_answer_body", { id, pretty });
  return answer instanceof ArrayBuffer ? new Uint8Array(answer) : Uint8Array.from(answer);
}

/** What was sent from `file` since knobas started, the newest first. */
export function apiSent(file: string): Promise<SentLine[]> {
  return invoke<SentLine[]>("api_sent", { file });
}

/** Holds a secret typed on the page for `origin` and `scheme`, until knobas quits. */
export function apiHoldSecret(origin: string, scheme: string, secret: string, user: string | null): Promise<void> {
  return invoke<void>("api_hold_secret", { origin, scheme, secret, user });
}

/** Which schemes a secret is held for at `origin`: names only. */
export function apiHeld(origin: string): Promise<string[]> {
  return invoke<string[]>("api_held", { origin });
}

/** Forgets every secret held for `origin`. */
export function apiForgetHeld(origin: string): Promise<void> {
  return invoke<void>("api_forget_held", { origin });
}

/** The saved servers. */
export function apiServers(): Promise<SavedServers> {
  return invoke<SavedServers>("api_servers", {});
}

/** Saves the whole list of servers; answers it as stored. */
export function apiSetServers(servers: SavedServers): Promise<SavedServers> {
  return invoke<SavedServers>("api_set_servers", { servers });
}

/** Saves the body of send `id` to the Downloads folder under `name`; answers the path. */
export function apiSaveAnswer(id: number, name: string): Promise<string> {
  return invoke<string>("api_save_answer", { id, name });
}

/** Opens a spec in an application with an API spec command; answers what ran and the folder. */
export function apiOpenIn(application: string, spec: SpecToOpen): Promise<OpenedSpec> {
  return invoke<OpenedSpec>("api_open_in", { application, spec });
}

/** Which shipped API clients are on this computer. */
export function apiClientsFound(): Promise<ClientsFound> {
  return invoke<ClientsFound>("api_clients_found", {});
}

/** How many bytes the API clients' scratch folders hold. */
export function apiScratchSize(): Promise<number> {
  return invoke<number>("api_scratch_size", {});
}

/** Removes every API client scratch folder. */
export function apiScratchClear(): Promise<void> {
  return invoke<void>("api_scratch_clear", {});
}

/** An image at an address in a document, loaded on the press that asks (the XML view, section 14): one `GET`, no cookie. */
export function imageFetch(url: string): Promise<FetchedImage> {
  return invoke<FetchedImage>("image_fetch", { url });
}

/** The hosts whose images load when their dialog opens. */
export function imageHosts(): Promise<ImageHosts> {
  return invoke<ImageHosts>("image_hosts");
}

/** Stores the hosts whose images load when their dialog opens; answers them as stored. */
export function setImageHosts(hosts: ImageHosts): Promise<ImageHosts> {
  return invoke<ImageHosts>("set_image_hosts", { hosts });
}

/** The schema files a person pointed to, and the addresses downloaded (ADR-0058, `xsd.locations`). */
export function xsdLocations(): Promise<SchemaLocations> {
  return invoke<SchemaLocations>("xsd_locations");
}

/** Keeps the schema files pointed to; answers them as kept. */
export function xsdSetLocations(list: SchemaLocations): Promise<SchemaLocations> {
  return invoke<SchemaLocations>("xsd_set_locations", { list });
}

/**
 * The `.xsd` files of the synced repositories that declare `namespace` (the XSD design, section 16, step 6), from the
 * files index: the shortest paths first, at most 50. Empty where the index holds no file.
 */
export function filesDeclaring(namespace: string): Promise<DeclaringFile[]> {
  return invoke<DeclaringFile[]>("files_declaring", { namespace });
}

/** Downloads the schema at `address` on a press into the profile's `schemas/` (the XSD design, section 17). */
export function xsdFetch(address: string): Promise<DownloadedSchema> {
  return invoke<DownloadedSchema>("xsd_fetch", { address });
}

/** A downloaded schema's text by its SHA-256; `null` where it is not kept. */
export function xsdDownloaded(sha256: string): Promise<string | null> {
  return invoke<string | null>("xsd_downloaded", { sha256 });
}

/** Removes a downloaded schema's file. */
export function xsdForget(sha256: string): Promise<void> {
  return invoke<void>("xsd_forget", { sha256 });
}

/** *Save to Downloads*: an example made from a schema written to the Downloads folder under `name`; answers the path. */
export function xsdSaveExample(name: string, text: string): Promise<string> {
  return invoke<string>("xsd_save_example", { name, text });
}

/**
 * A file pipeline over a document knobas holds in memory (an example opened in the XML view, step E1): `steps` over
 * `text`'s bytes as the file `name`, as `entityFilePiped` runs them over a file's. A step that fails is an answer.
 */
export function textPiped(text: string, name: string, steps: PipeStep[]): Promise<PipeRun> {
  return invoke<PipeRun>("text_piped", { text, name, steps });
}

/** *Download* in the value viewer over a document held in memory: {@link textPiped}'s answer saved as `saveAs`. */
export function textPipedSave(text: string, name: string, steps: PipeStep[], saveAs: string): Promise<string> {
  return invoke<string>("text_piped_save", { text, name, steps, saveAs });
}

/**
 * Opens a jq session over a JSON document (the XML view's design, section 26): a file read as `entity_file_bytes`
 * reads it, or `text` where the view draws something else than the file. Answers its id.
 */
export function jsonQueryOpen(source: { entityId: string; path: string; commit: string | null } | { text: string }): Promise<number> {
  return invoke<number>("json_query_open", "text" in source ? { text: source.text } : { entityId: source.entityId, path: source.path, commit: source.commit });
}

/** A jq filter asked of session `id`: its first values, their count, and the paths of what it picked. */
export function jsonQuery(id: number, filter: string): Promise<JsonQueryAnswer> {
  return invoke<JsonQueryAnswer>("json_query", { id, filter });
}

/** Ends session `id` and its helper. */
export function jsonQueryClose(id: number): Promise<void> {
  return invoke<void>("json_query_close", { id });
}

/** Whether an XML file is checked against its schema when one is found (`xml.check`, on by default). */
export function xmlCheck(): Promise<XmlCheck> {
  return invoke<XmlCheck>("xml_check");
}

/** Stores whether XML files are checked. */
export function setXmlCheck(check: XmlCheck): Promise<XmlCheck> {
  return invoke<XmlCheck>("set_xml_check", { check });
}
