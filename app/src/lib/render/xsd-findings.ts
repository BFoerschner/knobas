/**
 * A departure in knobas's words (the XSD design, 2026-11-23, section 26): one finding of libxml2 said the way knobas
 * says things. Names lose their `{namespace}` and take the set's prefix; what libxml2 says that knobas has no sentence
 * for is kept whole, as libxml2 wrote it.
 */
import type { Run } from "./xsd-answer";

/** libxml2's finding as it crosses from the check: its sentence, its line (16 bits) and its path to the node. */
export interface Finding {
  readonly message: string;
  readonly line: number;
  readonly xpath: string | null;
}

export type FindingKind = "unexpected" | "missing" | "value" | "root" | "other";

export interface Words {
  /** The element (and the attribute) it is about, as the document writes it. */
  readonly at: string;
  readonly attr: string | null;
  readonly kind: FindingKind;
  /** The sentence; `raw` where knobas has none. */
  readonly text: readonly Run[];
  readonly raw: string;
  /** Whether knobas has a sentence for it. */
  readonly said: boolean;
  readonly expected: readonly string[];
  readonly value: string | null;
}

const MAX_NAMED = 6;

/**
 * The words of one finding. `label(ns, local)` writes a name as the set's pages do (`label` in `xsd.ts`): bare in the
 * schema's own namespace, with its prefix elsewhere.
 */
export function findingWords(finding: Finding, label: (ns: string, local: string) => string): Words {
  const raw = finding.message.trim();
  const short = (q: string): string => {
    const m = /^\{([^}]*)\}(.+)$/.exec(q.trim());
    return m === null ? q.trim() : label(m[1]!, m[2]!);
  };
  const names = (list: string) =>
    list
      .split(",")
      .map((s) => short(s))
      .filter((s) => s !== "");
  const head = /^Element '([^']+)'(?:, attribute '([^']+)')?: (.*)$/s.exec(raw);
  const base = { at: head ? short(head[1]!) : "", attr: head?.[2] !== undefined ? short(head[2]) : null, raw, expected: [] as string[], value: null as string | null };
  if (head === null) return { ...base, kind: "other", text: [{ text: raw }], said: false };
  const rest = head[3]!;
  const subject: Run = { text: base.attr !== null ? `@${base.attr}` : base.at, code: true };
  const code = (text: string): Run => ({ text, code: true });
  const join = (list: readonly string[], last: string): Run[] => list.flatMap((n, i) => [...(i === 0 ? [] : [{ text: i === list.length - 1 ? last : ", " }]), code(n)]);
  let m: RegExpExecArray | null;
  if ((m = /^This element is not expected\.(?: Expected is (?:one of )?\( (.*) \)\.)?$/s.exec(rest))) {
    const expected = m[1] !== undefined ? names(m[1]).filter((e) => !e.startsWith("##")) : [];
    const text: Run[] =
      expected.length === 0
        ? [subject, { text: " may not stand here." }]
        : expected.length === 1
          ? [subject, { text: " stands where " }, code(expected[0]!), { text: " is expected." }]
          : [subject, { text: " may not stand here. Expected: " }, ...join(expected.slice(0, MAX_NAMED), ", "), { text: expected.length > MAX_NAMED ? ` or one of ${expected.length - MAX_NAMED} more.` : "." }];
    return { ...base, kind: "unexpected", text, said: true, expected };
  }
  if ((m = /^Missing child element\(s\)\. Expected is (?:one of )?\( (.*) \)\.$/s.exec(rest))) {
    const expected = names(m[1]!);
    return { ...base, kind: "missing", text: [subject, { text: " ends too soon: " }, ...join(expected.slice(0, MAX_NAMED), " or "), { text: " is missing." }], said: true, expected };
  }
  if ((m = /^\[facet 'enumeration'\] The value '(.*)' is not an element of the set \{(.*)\}\.$/s.exec(rest))) {
    const n = m[2]!.split("', '").length;
    return { ...base, kind: "value", value: m[1]!, text: [code(m[1]!), { text: ` is not one of the ${n.toLocaleString("en")} values ` }, subject, { text: " may hold." }], said: true };
  }
  if ((m = /^\[facet 'pattern'\] The value '(.*)' is not accepted by the pattern '(.*)'\.$/s.exec(rest))) {
    const p = m[2]!;
    return { ...base, kind: "value", value: m[1]!, text: [code(m[1]!), { text: " does not match the pattern " }, subject, { text: " is held to, " }, code(p.length > 60 ? p.slice(0, 57) + "…" : p), { text: "." }], said: true };
  }
  if ((m = /^\[facet '(maxLength|minLength|length)'\] The value has a length of '(\d+)'; this (?:exceeds the allowed maximum length of|underruns the allowed minimum length of|differs from the allowed length of) '(\d+)'\.$/s.exec(rest))) {
    const may = m[1] === "maxLength" ? `${m[3]} at most` : m[1] === "minLength" ? `no shorter than ${m[3]}` : `exactly ${m[3]}`;
    return { ...base, kind: "value", text: [subject, { text: ` is ${m[2]} characters long; it may be ${may}.` }], said: true };
  }
  if ((m = /^\[facet '(minInclusive|maxInclusive|minExclusive|maxExclusive)'\] The value '(.*)' is (?:less than|greater than)[^']*'(.*)'\)?\.$/s.exec(rest))) {
    const bound = ({ minInclusive: "at least", maxInclusive: "at most", minExclusive: "more than", maxExclusive: "less than" } as Record<string, string>)[m[1]!]!;
    return { ...base, kind: "value", value: m[2]!, text: [code(m[2]!), { text: " is out of range for " }, subject, { text: `: ${bound} ` }, code(m[3]!), { text: "." }], said: true };
  }
  if ((m = /^'(.*)' is not a valid value of the (atomic|list|union) type '([^']+)'\.$/s.exec(rest))) {
    return { ...base, kind: "value", value: m[1]!, text: [code(m[1]!), { text: " is not a " }, code(short(m[3]!)), { text: ", which is what " }, subject, { text: " holds." }], said: true };
  }
  if ((m = /^The attribute '([^']+)' is required but missing\.$/s.exec(rest))) {
    return { ...base, kind: "missing", text: [subject, { text: " lacks its attribute " }, code(short(m[1]!)), { text: "." }], said: true };
  }
  if ((m = /^The attribute '([^']+)' is not allowed\.$/s.exec(rest))) {
    return { ...base, kind: "unexpected", text: [code(base.at), { text: " has an attribute the schema does not know, " }, code(short(m[1]!)), { text: "." }], said: true };
  }
  if (/^Character content other than whitespace is not allowed because the content type is 'element-only'\.$/.test(rest)) {
    return { ...base, kind: "unexpected", text: [subject, { text: " holds text, and may hold only elements." }], said: true };
  }
  if (/^Element content is not allowed, because the (content type is a simple type definition|type definition is simple)\.$/.test(rest)) {
    return { ...base, kind: "unexpected", text: [subject, { text: " holds elements, and may hold only a value." }], said: true };
  }
  if (/^No matching global declaration available for the validation root\.$/.test(rest)) {
    return { ...base, kind: "root", text: [{ text: "The schema declares no element " }, subject, { text: " a document may start with." }], said: true };
  }
  if ((m = /^Duplicate key-sequence \[(.*)\] in (unique|key) identity-constraint '([^']+)'\.$/s.exec(rest))) {
    return { ...base, kind: "value", text: [code(m[1]!), { text: " stands twice where " }, code(short(m[3]!)), { text: " asks for one of each." }], said: true };
  }
  if ((m = /^No match found for key-sequence \[(.*)\] of keyref '([^']+)'\.$/s.exec(rest))) {
    return { ...base, kind: "value", text: [code(m[1]!), { text: " names nothing: " }, code(short(m[2]!)), { text: " asks for a key the document holds." }], said: true };
  }
  if ((m = /^The value '(.*)' does not match the fixed value constraint '(.*)'\.$/s.exec(rest))) {
    return { ...base, kind: "value", value: m[1]!, text: [subject, { text: " must be " }, code(m[2]!), { text: "; it is " }, code(m[1]!), { text: "." }], said: true };
  }
  return { ...base, kind: "other", text: [{ text: raw, code: true }], said: false };
}

/** The whole sentence as plain text: a rail row's title, a test's comparison. */
export const plain = (runs: readonly Run[]): string => runs.map((r) => r.text).join("");
