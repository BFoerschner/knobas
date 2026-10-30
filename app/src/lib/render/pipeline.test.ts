import { expect, test } from "vitest";

import { STEP_CHOICES, decompressorFor, inputOf, moveStep, nameAfter, stepName } from "./pipeline";

test("an unwrap step takes its extension off the name, as the backend renames the output", () => {
  expect(nameAfter("fixtures/events.json.gz", [{ step: "gunzip" }])).toBe("fixtures/events.json");
  expect(nameAfter("a.tgz", [{ step: "gunzip" }])).toBe("a.tar");
  expect(nameAfter("hooks.json.b64", [{ step: "base64_decode" }])).toBe("hooks.json");
  expect(nameAfter("a.json.gz", [{ step: "unxz" }])).toBe("a.json.gz");
});

test("the decompress row picks the decompressor the extension names, gzip otherwise", () => {
  expect(decompressorFor("a.zst")).toBe("unzstd");
  expect(decompressorFor("a.tar.XZ")).toBe("unxz");
  expect(decompressorFor("a.bz2")).toBe("bunzip2");
  expect(decompressorFor("a.bin")).toBe("gunzip");
  const decompress = STEP_CHOICES.find((c) => c.id === "decompress")!;
  expect(decompress.make("a.json.zst")).toEqual({ step: "unzstd" });
  const convert = STEP_CHOICES.find((c) => c.id === "convert")!;
  expect(convert.make("a.yaml")).toEqual({ step: "convert", from: "yaml", to: "json" });
});

test("a step moves to its new place and the rest keep their order", () => {
  const steps = [{ step: "gunzip" }, { step: "jq", filter: "." }, { step: "hex_dump" }] as const;
  expect(moveStep(steps, 2, 0)).toEqual([steps[2], steps[0], steps[1]]);
  expect(moveStep(steps, 0, 3)).toEqual([...steps]);
});

test("a step is named as the backend names it, and a failed step's input says where it came from", () => {
  expect(stepName({ step: "convert", from: "json", to: "toml" })).toBe("JSON to TOML");
  expect(stepName({ step: "unzstd" })).toBe("zstd decompress");
  expect(inputOf([{ step: "gunzip" }], 0)).toBe("the file's bytes");
  expect(inputOf([{ step: "gunzip" }, { step: "jq", filter: "." }], 1)).toBe("the output of step 1 (gunzip)");
});
