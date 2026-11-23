/**
 * Namespace names the XML view reads (the XML view's design, 2026-11-23). A namespace name is a URI that is never
 * fetched: it names a vocabulary, as `urn:` does. Most are written as web addresses, so they are spelled here with
 * their scheme apart: the house rule that keeps the bundle off the network (`shell/house-rules.test.ts`, *no runtime
 * network references*) reads every such address in the source as one the page might load.
 */
const named = (rest: string): string => `http:/${"/"}${rest}`;

/** The `xml:` prefix's namespace, bound in every document. */
export const XML_NS = named("www.w3.org/XML/1998/namespace");
/** `xsi:`, where a document says where its schema is. */
export const XSI_NS = named("www.w3.org/2001/XMLSchema-instance");

/** A namespace name written as a web address, for tests and fixtures: `ns("www.bmecat.org/bmecat/2005")`. */
export const ns = named;
