/** An answer's words (ux31, section 19): the tone a status takes, the size in words, its file name and its view (#2533). */
import { expect, test } from "vitest";

import { answerName, bodyKind, sizeWords, statusTone } from "./oas-answer";

test("a status takes its tone: ok under 300, warn under 400, err from 400", () => {
  expect([200, 204, 299, 300, 304, 399, 400, 404, 500].map(statusTone)).toEqual(["ok", "ok", "ok", "warn", "warn", "warn", "err", "err", "err"]);
});

test("a size in words", () => {
  expect([0, 1023, 1024, 2150, 3 * 1024 * 1024].map(sizeWords)).toEqual(["0 B", "1023 B", "1.0 KB", "2.1 KB", "3.0 MB"]);
});

test("an answer goes by a file name its content type picks, for the pipeline's defaults and Save (#2533)", () => {
  expect(answerName(200, "application/json; charset=utf-8")).toBe("answer-200.json");
  expect(answerName(201, "application/problem+json")).toBe("answer-201.json");
  expect(answerName(200, "application/xml")).toBe("answer-200.xml");
  expect(answerName(404, "application/vnd.pets+xml")).toBe("answer-404.xml");
  expect(answerName(200, "text/html")).toBe("answer-200.html");
  expect(answerName(200, "text/plain")).toBe("answer-200.txt");
  expect(answerName(200, "application/gzip")).toBe("answer-200.gz");
  expect(answerName(200, "application/zstd")).toBe("answer-200.zst");
  expect(answerName(200, "application/octet-stream")).toBe("answer-200.bin");
  expect(answerName(204, "")).toBe("answer-204.bin");
});

test("an answer is drawn by the view its content type names, else what its text is; HTML is only ever its source (#2533)", () => {
  expect(bodyKind("application/json", "answer-200.json", "{}")).toBe("json");
  expect(bodyKind("application/hal+json", "answer-200.json", "{}")).toBe("json");
  expect(bodyKind("text/xml", "answer-200.xml", "<a/>")).toBe("xml");
  expect(bodyKind("application/atom+xml", "answer-200.xml", "<feed/>")).toBe("xml");
  // HTML never renders, not even as an XML document: it is text.
  expect(bodyKind("text/html", "answer-200.html", "<?xml version=\"1.0\"?><html><script>x()</script></html>")).toBe("text");
  expect(bodyKind("application/xhtml+xml", "answer-200.xml", "<html/>")).toBe("text");
  // A body whose type says nothing is read for what it is.
  expect(bodyKind("text/plain", "answer-200.txt", ' \n{"a": 1}')).toBe("json");
  expect(bodyKind("text/plain", "answer-200.txt", '<?xml version="1.0"?><a/>')).toBe("xml");
  expect(bodyKind("text/plain", "answer-200.txt", "hello")).toBe("text");
  // After a pipeline, the run's name speaks: gunzip made `answer-200.json` of `answer-200.json.gz`.
  expect(bodyKind("application/gzip", "answer-200.json", "[1]")).toBe("json");
});
