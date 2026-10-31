import { describe, expect, test } from "vitest";

import { STEP_CHOICES, decompressorFor, inputOf, moveStep, nameAfter, pipelinePattern, savedAs, stepName, suggestion } from "./pipeline";

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

const UPPER = { id: "upper", name: "Upper", command: "tr a-z A-Z", timeout_secs: 10 };

test("a command step goes by its saved name, or its id once deleted (#2265)", () => {
  expect(stepName({ step: "command", id: "upper" }, [UPPER])).toBe("Upper");
  expect(stepName({ step: "command", id: "gone" }, [UPPER])).toBe("gone");
  expect(inputOf([{ step: "command", id: "upper" }, { step: "gunzip" }], 1, [UPPER])).toBe("the output of step 1 (Upper)");
});

test("a pipeline is saved as the default for the file's last two extensions", () => {
  expect(pipelinePattern("fixtures/events.json.gz")).toBe("*.json.gz");
  expect(pipelinePattern("a/b.min.JS")).toBe("*.min.js");
  expect(pipelinePattern("blob.b64")).toBe("*.b64");
  expect(pipelinePattern("x.tar.json.gz")).toBe("*.json.gz");
  expect(pipelinePattern("Justfile")).toBe("Justfile");
});

describe("suggestion (#2265)", () => {
  const gz = Uint8Array.of(0x1f, 0x8b, 0x08, 0, 0);
  const zst = Uint8Array.of(0x28, 0xb5, 0x2f, 0xfd, 0);
  test("gzip and zstd bytes suggest their unwrap", () => {
    expect(suggestion({ binary: true, text: "" }, gz)).toEqual({ step: { step: "gunzip" }, kind: "gzip-compressed" });
    expect(suggestion({ binary: true, text: "" }, zst)).toEqual({ step: { step: "unzstd" }, kind: "zstd-compressed" });
    expect(suggestion({ binary: true, text: "" }, Uint8Array.of(0x89, 0x50, 0x4e, 0x47))).toBeNull();
    expect(suggestion({ binary: true, text: "" }, null)).toBeNull();
  });
  test("text that is all base64 suggests decoding it; prose, hex and short words do not", () => {
    const b64 = btoa(JSON.stringify({ events: [{ at: "08:14", level: "info" }] }));
    expect(suggestion({ binary: false, text: `${b64}\n` }, null)).toEqual({ step: { step: "base64_decode" }, kind: "base64" });
    expect(suggestion({ binary: false, text: `${b64.slice(0, 40)}\n${b64.slice(40)}` }, null)?.kind).toBe("base64");
    expect(suggestion({ binary: false, text: "hello world, this is prose\n" }, null)).toBeNull();
    expect(suggestion({ binary: false, text: "deadbeefdeadbeefdeadbeefdeadbeef" }, null)).toBeNull();
    expect(suggestion({ binary: false, text: "abcd" }, null)).toBeNull();
    expect(suggestion({ binary: false, text: "SGVsbG8gV29ybGQ" }, null)).toBeNull();
  });
});

test("saving under a pipeline's name updates it in place; a new name adds one", () => {
  const list = [{ id: "gz", name: "Gzipped JSON", steps: [{ step: "gunzip" as const }] }];
  const steps = [{ step: "gunzip" as const }, { step: "jq" as const, filter: ".a" }];
  expect(savedAs(list, " gzipped json ", steps)).toEqual([{ id: "gz", name: "Gzipped JSON", steps }]);
  expect(savedAs(list, "Events", steps)).toEqual([
    { id: "gz", name: "Gzipped JSON", steps: [{ step: "gunzip" }] },
    { id: null, name: "Events", steps },
  ]);
});
