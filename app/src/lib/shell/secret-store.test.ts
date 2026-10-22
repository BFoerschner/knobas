import { expect, test } from "vitest";
import { readSecretsInMemory, storedWhere, tokenKeptHint } from "./secret-store";

test("the words follow the store", () => {
  expect(storedWhere(true)).toBe("Stored for this session only");
  expect(storedWhere(false)).toMatch(/^Stored in the (macOS|system) keychain$/);
  expect(tokenKeptHint(true)).toContain("for this session only");
  expect(tokenKeptHint(true)).not.toContain("keychain");
  expect(tokenKeptHint(false)).toContain("in this machine's keychain");
});

test("a failed app_status reads as the keychain", async () => {
  expect(await readSecretsInMemory(() => Promise.reject(new Error("no")))).toBe(false);
  expect(await readSecretsInMemory(() => Promise.resolve({ secrets_in_memory: true } as never))).toBe(true);
});
