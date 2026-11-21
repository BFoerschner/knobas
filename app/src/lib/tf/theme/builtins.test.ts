/** The seven themes that come with knobas, each a pair (ADR-0055, the decisions' 3.13). */
import { join } from "node:path";

import { expect, test } from "vitest";

import { failures, fixAll, unreadable } from "./app-check";
import { derive, KNOBAS } from "./app-derive";
import { appFromVsCode } from "./app-import";
import { BUILTINS } from "./builtins";
import { hasApp, hasCode } from "./themes";

test("every built-in has both parts, passes every check of its window and is readable", () => {
  expect(BUILTINS).toHaveLength(7);
  for (const theme of BUILTINS) {
    expect(hasApp(theme), theme.id).toBe(true);
    expect(hasCode(theme), theme.id).toBe(true);
    const { colours } = derive(theme.kind, theme.app);
    expect(failures(colours), theme.id).toEqual([]);
    expect(unreadable(colours), theme.id).toEqual([]);
    // knobas's yellow card in all seven.
    expect([colours.card, colours.card2, colours.cardInk], theme.id).toEqual([KNOBAS[theme.kind].card, KNOBAS[theme.kind].card2, KNOBAS[theme.kind].cardInk]);
  }
  expect(BUILTINS.find((t) => t.id === "knobas-light")!.app).toEqual({});
  expect(BUILTINS.find((t) => t.id === "knobas-dark")!.app).toEqual({});
});

/** Shiki's file of each, the copies `docs/third-party-notices.md` names. */
const SHIKI_ID: Record<string, string> = { "github-light": "github-light", "github-dark": "github-dark", "one-dark": "one-dark-pro", dracula: "dracula", "catppuccin-mocha": "catppuccin-mocha" };

test("the five others' window colours are their VS Code themes read, then Fix all, with nothing changed by hand", async () => {
  for (const [id, file] of Object.entries(SHIKI_ID)) {
    const json = (await import(/* @vite-ignore */ join(process.cwd(), "node_modules/@shikijs/themes/dist", `${file}.mjs`))).default as Record<string, unknown>;
    const { kind, set } = appFromVsCode(json);
    const theme = BUILTINS.find((t) => t.id === id)!;
    expect(kind, id).toBe(theme.kind);
    expect(fixAll(kind, set), id).toEqual(theme.app);
  }
});
