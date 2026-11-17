/**
 * The fast YAML reader (ux18) gives exactly what js-yaml gives, or nothing: every spec fixture and every shape a spec
 * tool writes read the same both ways, and every shape it does not read for certain answers `undefined`, so js-yaml
 * reads it.
 */
import { JSON_SCHEMA, load } from "js-yaml";
import { expect, test } from "vitest";

import { fastYaml } from "./yaml-fast";
import { BROKEN_YAML, COMPOSE_YAML, ITEMS_YAML, ORDER_YAML, PETS_30_YAML, SPLIT_31_YAML, bigSpec } from "./openapi-fixture";

const slow = (text: string) => load(text, { schema: JSON_SCHEMA, json: true });

/** Read the same both ways, and read by the fast reader at all. */
function same(text: string) {
  const fast = fastYaml(text);
  expect(fast, `not read fast:\n${text}`).not.toBeUndefined();
  expect(fast).toEqual(slow(text));
}

test("every spec fixture reads the same as js-yaml reads it, the 5,000-operation one too", () => {
  for (const text of [PETS_30_YAML, SPLIT_31_YAML, ORDER_YAML, ITEMS_YAML, COMPOSE_YAML, bigSpec(5000)]) same(text);
});

test("the shapes spec tools write read the same", () => {
  same(`# a comment\n---\nopenapi: 3.0.0 # trailing\ninfo:\n  title: "A \\"quoted\\" \\u00e9 title"\n  version: '1.0 ''beta'''\n`);
  same(`a:\n- x\n- y\nb:\n  - 1\n  - -2\n  - 3.5\n  - 1e3\n  - true\n  - null\n  - ~\n  - .inf\n  - 0x1F\n  - 1.0.0\n  - -0\n  - 9007199254740993\n  - 1.5e-3\n`);
  same(`"200":\n  description: OK\n'404': {description: Missing}\n200: x\n`);
  same(`list:\n  - name: id\n    in: path\n    schema: { type: string, enum: [a, "b", 'c'] }\n  -   name: two\n      required: true\n  - plain\n  - [1, [2, {x: y}]]\n  -\n    nested: yes\n`);
  same(`d: |\n  line one\n    indented\n\n  after a blank\nk: |-\n  kept\n\n\np: |+\n  plus\n\nf: >\n  folded\n  together\n\n  paragraph\ng: >-\n  folded strip\nlast: x\n`);
  same(`url: https://example.com/a#frag\nref: { $ref: "#/components/schemas/X" }\nempty:\nempty2: # comment\nnext: [ ]\nobj: {}\n`);
  same(`a:\n  b:\n    c:\n      - d: 1\n        e:\n          - f\n  g: h\n`);
  same(`key with spaces: value with spaces\nx-ext:colon: v\n'': empty key\n`);
  same(`literal: |\n\n  starts blank\n`);
  same(`e:\n  -\n  - a\n  -\nf: 1\n`);
});

test("what it does not read for certain it leaves to js-yaml", () => {
  const unsure = [
    `a: &x 1\nb: *x\n`,
    `a: &x 1\n`,
    `a: 1\nb: [-0, 9007199254740993]\n`.replace("1\nb", "1\nb: {c: &y 2}\nd"),
    `a: !!str 1\n`,
    `a: "over\n  two lines"\n`,
    `a: plain\n  continued\n`,
    `a: [1,\n  2]\n`,
    `a: |2\n   x\n`,
    `a:\n\tb: 1\n`,
    `a: 1\n---\nb: 2\n`,
    `1.0: x\n`,
    `? complex\n: key\n`,
    `__proto__: x\n`,
    `<<: {a: 1}\n`,
    `a: b: c\n`,
    `a:\n  b: 1\n c: 2\n`,
    `- a\n- b\n`.replace("- a", "- - a"),
    `plain scalar at the top\n`,
    `a: "\\e"\n`,
    `a: >\n\n  folded after a blank\n`,
    `a: >\n  x\n    more indented\n`,
    `a: [a: b]\n`,
    `a: x\t# tab comment\n`,
  ];
  for (const text of unsure) expect(fastYaml(text), text).toBeUndefined();
  // And js-yaml reads or refuses each as before: the fast reader never answers for a broken file.
  expect(fastYaml(BROKEN_YAML)).toBeUndefined();
  expect(() => slow(BROKEN_YAML)).toThrow();
});

test("a sequence at the top and a document marker read the same", () => {
  same(`---\n- a: 1\n- b\n`);
});
