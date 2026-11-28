/**
 * Builds' *Recent* rules (2026-11-25): its order, what a read again of its head keeps, what the backend is asked, the
 * quiet text after a pipeline's name and what typed words keep.
 */
import { expect, test } from "vitest";

import type { RecentPipeline, RecentRun, RunRow } from "../../ipc/entity";
import { pipelineHash } from "./builds";
import { runText } from "./pipeline";
import {
  RECENT,
  RECENT_HASH,
  RECENT_HOLD,
  folderPipelines,
  isRecent,
  isRunScopeHash,
  runScopeHash,
  runScopeOf,
  folderColumn,
  nameParts,
  rowPipeline,
  placeWords,
  recentFilterOf,
  recentMarks,
  recentText,
  recentOrder,
  withHead,
  withOlder,
  withPipelines,
  withRun,
} from "./recent";

function run(id: string, started: string | null, extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: id,
    number: id.split(":").at(-1) ?? null,
    state: { text: "SUCCESS", tone: "ok" },
    branch: "main",
    started,
    finished: started,
    duration_seconds: 60,
    updated_at: "2026-08-22T00:00:00Z",
    web_url: null,
    trigger: null,
    started_by: null,
    started_by_me: false,
    revision: null,
    commit_id: null,
    agent: null,
    current_step: null,
    tests: null,
    ...extra,
  };
}

const at = (id: string, hour: number, pipeline = "tc:buildType:Lint"): RecentRun => ({
  pipeline_id: pipeline,
  run: run(`tc:build:${id}`, `2026-08-22T${String(hour).padStart(2, "0")}:00:00Z`),
});

const ids = (runs: readonly RecentRun[]) => runs.map((one) => one.run.entity_id.split(":").at(-1));

const lint: RecentPipeline = { entity_id: "tc:buildType:Lint", source_id: "tc", source_name: "Tidewater TeamCity", name: "Lint", group: "Payout", path: ["Payments", "Payout"] };

test("the address is a word no pipeline's id is", () => {
  expect(RECENT_HASH).toBe("#/builds/recent");
  expect(isRecent([RECENT])).toBe(true);
  expect(isRecent(["recent", "runs"])).toBe(false);
  expect(isRecent(["tc:buildType:recent"])).toBe(false);
  // Every pipeline's address names its source: a pipeline can never be the word.
  expect(pipelineHash("tc:buildType:recent")).not.toBe(RECENT_HASH);
});

test("newest first by when a run started, a queued one by when it was queued, the greater id on a tie", () => {
  const long = run("tc:build:1", "2026-08-22T08:00:00Z", { finished: "2026-08-22T12:00:00Z", updated_at: "2026-08-22T12:00:00Z" });
  const short = run("tc:build:2", "2026-08-22T09:00:00Z");
  const queued = run("tc:build:3", null, { finished: null, updated_at: "2026-08-22T10:00:00Z" });
  const twin = run("tc:build:4", "2026-08-22T09:00:00Z");
  expect([long, short, queued, twin].sort(recentOrder).map((one) => one.entity_id)).toEqual(["tc:build:3", "tc:build:4", "tc:build:2", "tc:build:1"]);
  expect(recentOrder(short, short)).toBe(0);
});

test("a head read again keeps the older runs it held, and drops what left inside its span", () => {
  const held = [at("9", 9), at("8", 8), at("7", 7), at("6", 6), at("5", 5)];
  // The head of two: a new run at 10, 9 as it was; 8 is no longer in it, but past its span it stays held.
  const head = [at("10", 10), at("9", 9)];
  expect(ids(withHead(held, head, false))).toEqual(["10", "9", "8", "7", "6", "5"]);
  // 9 left within the head's span: it goes.
  expect(ids(withHead(held, [at("10", 10), at("8", 8)], false))).toEqual(["10", "8", "7", "6", "5"]);
  // A whole answer holds alone.
  expect(ids(withHead(held, head, true))).toEqual(["10", "9"]);
  expect(withHead(held, [], false)).toEqual([]);
});

test("an older page follows what is held, nothing twice, never past the hold", () => {
  expect(ids(withOlder([at("9", 9), at("8", 8)], [at("8", 8), at("7", 7)]))).toEqual(["9", "8", "7"]);
  const many = Array.from({ length: RECENT_HOLD + 10 }, (_, n) => at(String(n), 1));
  expect(withOlder([], many)).toHaveLength(RECENT_HOLD);
  expect(withHead([], many, true)).toHaveLength(RECENT_HOLD);
  expect(withHead(many.slice(0, 5), many, false)).toHaveLength(RECENT_HOLD);
});

test("a run read again at its source is laid in where it is held", () => {
  const moved = run("tc:build:8", "2026-08-22T08:00:00Z", { state: { text: "FAILURE", tone: "err" } });
  const after = withRun([at("9", 9), at("8", 8)], moved);
  expect(after[1]!.run.state?.tone).toBe("err");
  expect(after[0]!.run.state?.tone).toBe("ok");
});

test("the backend is asked the scope, the results and the source", () => {
  expect(recentFilterOf({ scope: "all", results: [], source: null }, null)).toEqual({ sources: [], results: [], mine: false, branches: null, pipelines: [] });
  expect(recentFilterOf({ scope: "mine", results: ["Failed", "Bogus"], source: "tc" }, null)).toEqual({
    sources: ["tc"],
    results: ["Failed"],
    mine: true,
    branches: null,
    pipelines: [],
  });
  expect(recentFilterOf({ scope: "job", results: [], source: null }, ["feature/PAY-231"]).branches).toEqual(["feature/PAY-231"]);
  // *This job* before its branches are read asks for none, not for every run.
  expect(recentFilterOf({ scope: "job", results: [], source: null }, null).branches).toEqual([]);
});

test("a folder's scope asks for its pipelines by id; every pipeline asks for none (#2570)", () => {
  const choice = { scope: "all", results: [], source: null };
  expect(recentFilterOf(choice, null, ["tc:buildType:B", "tc:buildType:A"]).pipelines).toEqual(["tc:buildType:A", "tc:buildType:B"]);
  expect(recentFilterOf(choice, null).pipelines).toEqual([]);
});

test("Recent and a folder are run scopes with addresses of their own, under words no pipeline's id is (#2570)", () => {
  expect(runScopeOf(["recent"])).toEqual({ kind: "all" });
  expect(runScopeOf(["folder", "tc", "Payments", "Payout"])).toEqual({ kind: "folder", path: ["tc", "Payments", "Payout"] });
  expect(runScopeOf(["folder", "~repo", "git:acme/pay"])).toEqual({ kind: "folder", path: ["~repo", "git:acme/pay"] });
  expect(runScopeOf(["folder"]), "a folder names its place").toBeNull();
  expect(runScopeOf(["tc:buildType:folder"])).toBeNull();
  expect(runScopeOf([])).toBeNull();
  expect(runScopeHash({ kind: "all" })).toBe(RECENT_HASH);
  // Each segment encoded: a group may hold a slash, a space or a `›`.
  const hash = runScopeHash({ kind: "folder", path: ["tc", "Pay/outs", "Nightly › EU"] });
  expect(hash).toBe("#/builds/folder/tc/Pay%2Fouts/Nightly%20%E2%80%BA%20EU");
  const back = hash.slice("#/builds/".length).split("/").map(decodeURIComponent);
  expect(runScopeOf(back)).toEqual({ kind: "folder", path: ["tc", "Pay/outs", "Nightly › EU"] });
  expect(isRunScopeHash("#/builds/recent")).toBe(true);
  expect(isRunScopeHash(hash)).toBe(true);
  expect(isRunScopeHash("#/builds/tc%3AbuildType%3ALint")).toBe(false);
  expect(isRunScopeHash("#/build/tc%3AbuildType%3ALint/tc%3Abuild%3A1")).toBe(false);
  expect(isRunScopeHash("#/builds")).toBe(false);
  // A pipeline's address names its source and a `:`, never the word.
  expect(pipelineHash("tc:buildType:folder").startsWith("#/builds/folder/")).toBe(false);
});

test("a folder lists every pipeline under it, by project from its source's groups, by repository from what its runs built", () => {
  const at = (id: string, source: string, path: string[]) => ({ entity_id: id, source_id: source, path, group: path.at(-1) ?? null });
  const list = [
    at("tc:a", "tc", ["Payments", "Payout"]),
    at("tc:b", "tc", ["Payments", "Payout", "Deploy"]),
    at("tc:c", "tc", ["Payments", "Payroll"]),
    at("tc:d", "tc", []),
    at("other:e", "other", ["Payments", "Payout"]),
  ];
  const repos = (p: { entity_id: string }) => (p.entity_id === "tc:a" ? [{ id: "git:pay" }] : p.entity_id === "tc:c" ? [{ id: "git:pay" }, { id: "git:roll" }] : []);
  expect(folderPipelines(list, ["tc", "Payments", "Payout"], repos)).toEqual(["tc:a", "tc:b"]);
  expect(folderPipelines(list, ["tc", "Payments"], repos)).toEqual(["tc:a", "tc:b", "tc:c"]);
  expect(folderPipelines(list, ["tc", "Pay"], repos), "a group, not a prefix of its name").toEqual([]);
  expect(folderPipelines(list, ["~repo", "git:pay"], repos)).toEqual(["tc:a", "tc:c"]);
  expect(folderPipelines(list, ["~repo"], repos), "No repository").toEqual(["tc:b", "tc:d", "other:e"]);
});

test("a pipeline's place is its folders, its source first where two or more list", () => {
  expect(placeWords(lint, false)).toBe("Payments › Payout");
  expect(placeWords(lint, true)).toBe("Tidewater TeamCity › Payments › Payout");
  expect(placeWords({ ...lint, path: [], group: "Payout" }, false)).toBe("Payout");
  expect(placeWords({ ...lint, path: [], group: null }, false)).toBe("");
  expect(placeWords(undefined, true)).toBe("");
});

test("a run's text is its own words, then its pipeline's name and place as the row draws them, and its source (#2535)", () => {
  const pipes = withPipelines(new Map(), [lint, { ...lint, entity_id: "tc:buildType:Deploy", name: "Deploy", path: ["Ops"] }]);
  const deploy = at("2", 2, "tc:buildType:Deploy");
  const own = runText(deploy.run);
  const name = own.length + 1;
  expect(recentText(deploy, pipes, false)).toBe(`${own} Deploy ${placeWords(pipes.get("tc:buildType:Deploy"), false)} Tidewater TeamCity`);
  expect(recentText(deploy, pipes, true)).toBe(`${own} Deploy ${placeWords(pipes.get("tc:buildType:Deploy"), true)}`);
  // "De" of the name and "Op" of its place, past the run's own words.
  const place = name + "Deploy".length + 1;
  const marks = recentMarks(deploy, pipes, false, [name, name + 2, place, place + 2]);
  expect(marks.name).toEqual([0, 2]);
  expect(marks.place).toEqual([0, 2]);
  expect(marks.branch).toEqual([]);
  expect(recentText({ pipeline_id: "gone", run: deploy.run }, pipes, false)).toBe(`${own}  `);
});

test("a page that names only pipelines held as they are keeps the held map, so the drawn rows are not drawn again (#2569b)", () => {
  const held = withPipelines(new Map(), [lint]);
  expect(withPipelines(held, [{ ...lint }])).toBe(held);
  expect(withPipelines(held, [])).toBe(held);
});

test("a page's pipelines are laid over those held", () => {
  const held = withPipelines(new Map(), [lint]);
  const after = withPipelines(held, [{ ...lint, name: "Lint again" }]);
  expect(after.get(lint.entity_id)?.name).toBe("Lint again");
  expect(held.get(lint.entity_id)?.name).toBe("Lint");
});

test("a pipeline's name is its last word and the lead before it, cut apart (#2572, the README's examples)", () => {
  expect(nameParts("CarrierAdapters_Contract_Verification_Nightly")).toEqual(["CarrierAdapters_Contract_Verification_", "Nightly"]);
  expect(nameParts("Payout_IntegrationTests")).toEqual(["Payout_", "IntegrationTests"]);
  expect(nameParts("CustomsGateway_Build")[1], "Build and Contract stay apart").toBe("Build");
  expect(nameParts("CustomsGateway_Contract")[1]).toBe("Contract");
  expect(nameParts("Deploy to staging")).toEqual(["Deploy to ", "staging"]);
  expect(nameParts("deploy-prod")).toEqual(["deploy-", "prod"]);
  expect(nameParts("payments.api.v2"), "a short last word still tells v2 from v3").toEqual(["payments.api.", "v2"]);
});

test("a name with no _, space, - or . splits at its last capital, else it is all last word (#2572's ruling)", () => {
  expect(nameParts("CustomsGatewayBuild")).toEqual(["CustomsGateway", "Build"]);
  expect(nameParts("APIGatewayTests")).toEqual(["APIGateway", "Tests"]);
  expect(nameParts("Build")).toEqual(["", "Build"]);
  expect(nameParts("nightly")).toEqual(["", "nightly"]);
  expect(nameParts("BUILD")).toEqual(["", "BUILD"]);
  expect(nameParts("_Build"), "a separator that leads nothing").toEqual(["", "_Build"]);
  expect(nameParts("Build_"), "a separator that ends it").toEqual(["", "Build_"]);
  expect(nameParts("")).toEqual(["", ""]);
});

test("at a folder a pipeline's place is said from that folder down, with no source (#2572)", () => {
  const deep = { ...lint, path: ["Payments", "Payout", "Nightly"] };
  expect(placeWords(deep, true, ["tc", "Payments"])).toBe("Payout › Nightly");
  expect(placeWords(deep, false, ["tc", "Payments", "Payout", "Nightly"]), "the folder itself: nothing").toBe("");
  expect(placeWords(deep, true, ["~repo", "git:pay"]), "a repository is none of its folders: said whole").toBe("Tidewater TeamCity › Payments › Payout › Nightly");
  expect(placeWords(deep, false, null)).toBe("Payments › Payout › Nightly");
});

test("at a folder a run's text and marks read the place as the row says it, from that folder down (#2572)", () => {
  const pipes = withPipelines(new Map(), [{ ...lint, entity_id: "tc:buildType:Deploy", name: "Deploy", path: ["Ops", "Prod"] }]);
  const deploy = at("2", 2, "tc:buildType:Deploy");
  const own = runText(deploy.run);
  expect(recentText(deploy, pipes, false, ["tc", "Ops"])).toBe(`${own} Deploy Prod Tidewater TeamCity`);
  // "Pr" of the place said at the folder: its first letters, not the fifth and sixth of "Ops › Prod".
  const place = own.length + 1 + "Deploy".length + 1;
  expect(recentMarks(deploy, pipes, false, [place, place + 2], ["tc", "Ops"]).place).toEqual([0, 2]);
});

test("a row's pipeline cell: its name, the folders it says, their link and the whole path as its title (#2572)", () => {
  const pipe = { ...lint, path: ["Platform", "Services", "Auth gateway"], name: "Build" };
  expect(rowPipeline(pipe, false, null)).toEqual({
    name: "Build",
    place: "Platform › Services › Auth gateway",
    folder: runScopeHash({ kind: "folder", path: ["tc", "Platform", "Services", "Auth gateway"] }),
    whole: "Platform › Services › Auth gateway › Build",
  });
  const below = rowPipeline(pipe, true, ["tc", "Platform"]);
  expect(below.place).toBe("Services › Auth gateway");
  expect(below.folder, "the link is the pipeline's own folder, wherever the table stands").toBe(rowPipeline(pipe, false, null).folder);
  expect(below.whole, "the title says the whole path, its source first where two list").toBe("Tidewater TeamCity › Platform › Services › Auth gateway › Build");
  expect(rowPipeline(pipe, false, ["tc", "Platform", "Services", "Auth gateway"]).folder, "no folders said, no link").toBeNull();
  expect(rowPipeline(undefined, false, null)).toEqual({ name: "", place: "", folder: null, whole: "" });
});

test("the folder column: wide at All pipelines, narrow at a folder with folders below it, gone without (#2572)", () => {
  const a = { ...lint, entity_id: "tc:a", path: ["Payments", "Payout"] };
  const b = { ...lint, entity_id: "tc:b", path: ["Payments"] };
  expect(folderColumn([a, b], null)).toBe("long");
  expect(folderColumn([a, b], ["tc", "Payments"])).toBe("short");
  expect(folderColumn([a], ["tc", "Payments", "Payout"])).toBe("none");
  expect(folderColumn([{ ...b, path: [], group: null }], ["~repo", "git:pay"]), "by repository the folders are said whole, a source's name too").toBe("long");
});
