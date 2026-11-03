/**
 * `?fake-ipc`: The Settings pages that read a stored preference (#831): Time (passive
 * attribution), Monitoring and Backup and export's status.
 *
 * Answers `user_renderers`, `set_user_renderers`, `run_renderer` (#2262: one saved
 * renderer, `cat`, which the fake runs by echoing its input; any other command
 * fails with that said on stderr), `renderer_defaults`, `set_renderer_defaults` (#2263, none until
 * one is made), `diff_pipelines`, `set_diff_pipelines` (#2270, on until set), `command_steps`, `set_command_steps`, `named_pipelines`, `set_named_pipelines` (#2265: one
 * command step, Upper, which `repoFilePiped` runs; no pipeline until one is saved, but under
 * `?fake-diff-pipeline` *Base64 JSON*, the default for `*.json.b64`), `pages_kept_open`, `set_pages_kept_open` (#2247), `passive_attribution`, `set_passive_attribution`,
 * `monitoring_settings`, `set_monitoring_settings`, `state_colours`, `set_tone_override`,
 * `clear_tone_overrides` (#2465: the adapters' declared words, two seen Jira statuses with their category tone (#2468), no
 * override until one is set), `backup_status`,
 * `backup_now`, `set_backup_schedule` and `open_backup_folder` (which opens
 * nothing: a browser has no file manager to show the folder in).
 * Notifications are `inbox.ts`'s and Standup's publish target is
 * `standup.ts`'s, each beside the data it names.
 *
 * **Values are prototype-6's `STGV` pages**: passive blocks on; slow above
 * 2000 ms and samples kept 30 days; the last backup "today 02:00, 48 MB",
 * every day at 02:00, keep 14.
 *
 * **Where this diverges from prototype-6:**
 *
 * - `share_export` and `restore_backup` stay unanswered, with the census's
 *   reason (the deputy's ruling of 2026-09-08 on #507: a fake would certify
 *   nothing about an archive). Prototype-6's *Export...* and *Restore from a
 *   file...* buttons therefore reject under `?fake-ipc`.
 * - Prototype-6 draws the schedule as "Every day at 02:00 / Every week /
 *   Never"; `BackupSchedule` is `{enabled, hour, minute, keep}`, so a weekly
 *   choice has no field (listed in the PR, not invented here).
 * - Prototype-6's "Back up" label is the nightly export; `backup_now` answers
 *   a record and adds an archive to the list, nothing more.
 */
import type { BackupRecord, BackupSchedule, BackupStatus } from "../../../ipc/backup";
import type { MonitoringSettings } from "../../../ipc/assets";
import type { CommandStep, CommandStepDraft, NamedPipeline, NamedPipelineDraft, RenderContent, RendererDefault, UserRenderer, UserRendererDraft } from "../../../ipc/app";
import type { KindStateColours, ToneChoice } from "../../../ipc/sources";
import type { Tone } from "../../../ipc/generated/entity";
import type { Handlers } from "../fake-tauri";

const DIRECTORY = "~/Library/Application Support/knobas/backups";

/** Today at `hour:minute` by the browser's clock, so "today 02:00" stays today. */
function todayAt(hour: number, minute: number): Date {
  const at = new Date();
  at.setHours(hour, minute, 0, 0);
  return at;
}

function archiveName(at: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `knobas-${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}-${pad(at.getHours())}${pad(at.getMinutes())}.knobas`;
}

function record(at: Date, bytes: number): BackupRecord {
  return { taken_at: at.toISOString(), file: `${DIRECTORY}/${archiveName(at)}`, bytes };
}

/** The fake's declared state words per kind, in declared order, and the Jira statuses it has seen. */
const DECLARED_STATES: [string, [string, Tone | null][]][] = [
  ["build", [["running", "info"], ["queued", "info"], ["SUCCESS", "ok"], ["FAILURE", "err"], ["UNKNOWN", "muted"]]],
  ["monitor", [["up", "ok"], ["down", "err"], ["pending", "warn"], ["maintenance", "muted"]]],
  ["pr", [["merged", "ok"], ["open", "info"], ["closed", "muted"]]],
  ["ticket", [["Done", "ok"], ["Resolved", "ok"], ["In Progress", "info"], ["In Review", "info"], ["Closed", "muted"]]],
];
/** Each seen status with its tickets' Jira category tone (#2468): Blocked is in progress, To Do is to do. */
const SEEN_STATES: Record<string, [string, Tone | null][]> = { ticket: [["Blocked", "info"], ["To Do", null]] };

/** `state_colours` over the fake's overrides, as `sources::state_colours::read` answers. */
function stateColours(overrides: Map<string, ToneChoice>): KindStateColours[] {
  return DECLARED_STATES.map(([kind, declared]) => {
    const yours = (word: string) => overrides.get(`${kind}\u0000${word}`) ?? null;
    const category = (word: string) => SEEN_STATES[kind]?.find(([w]) => w === word)?.[1] ?? null;
    const undeclared = [...(SEEN_STATES[kind] ?? []).map(([w]) => w), ...[...overrides.keys()].filter((key) => key.startsWith(`${kind}\u0000`)).map((key) => key.slice(kind.length + 1))]
      .filter((word, i, all) => kind in SEEN_STATES && all.indexOf(word) === i && !declared.some(([w]) => w === word))
      .sort();
    const words = [
      ...declared.map(([word, tone]) => ({ word, declared: true, declared_tone: tone, category_tone: null })),
      ...undeclared.map((word) => ({ word, declared: false, declared_tone: null, category_tone: category(word) })),
    ].map(({ word, declared, declared_tone, category_tone }) => {
      const mine = yours(word);
      const fallback = declared ? declared_tone : category_tone;
      const tone: Tone | null = mine === null ? fallback : mine === "none" ? null : mine;
      const origin = mine !== null ? "yours" : declared ? "adapter" : category_tone !== null ? "category" : "unset";
      return { word, declared, declared_tone, category_tone, yours: mine, tone, origin } as const;
    });
    return { kind, words };
  });
}

/** This feature's commands. */
export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  let passive = true;
  const toneOverrides = new Map<string, ToneChoice>();
  // `?fake-tone-override` (#2469): one word is already yours, so the share export draws *State colours*.
  if (params.has("fake-tone-override")) toneOverrides.set("ticket\u0000Blocked", "warn");
  // `?fake-pages-kept-open=<n>` starts *Pages kept open* at n (the keep-alive budget, #2250).
  const keptAsked = new URLSearchParams(globalThis.location?.search).get("fake-pages-kept-open");
  let keptOpen = keptAsked === null ? 5 : Math.min(15, Math.max(0, Math.round(Number(keptAsked)) || 0));
  let monitoring: MonitoringSettings = { sample_retention_days: 30, response_time_warn_ms: 2000 };
  let schedule: BackupSchedule = { enabled: true, hour: 2, minute: 0, keep: 14 };
  const taken = todayAt(2, 0);
  const MB = 1024 * 1024;
  const archives = [record(taken, 48 * MB)];
  for (const back of [1, 2, 3]) {
    const earlier = new Date(taken.getTime() - back * 86_400_000);
    archives.push(record(earlier, 47 * MB - back * 200_000));
  }

  const status = (): BackupStatus => {
    const next = todayAt(schedule.hour, schedule.minute);
    if (next.getTime() <= Date.now()) next.setDate(next.getDate() + 1);
    return {
      schedule,
      directory: DIRECTORY,
      last: archives[0] ?? null,
      next_due_at: schedule.enabled ? next.toISOString() : null,
      archives: archives.slice(0, Math.max(1, schedule.keep)).map((made) => ({
        file: made.file,
        bytes: made.bytes,
        share: false,
      })),
    };
  };

  let renderers: UserRenderer[] = [
    { id: "cat", name: "Cat", command: "cat", patterns: ["*.cat.html"], fences: ["cat"], input: "text", output: "html", timeout_secs: 10, diff: false },
  ];

  // The default renderer per file pattern (#2263): none until one is made.
  let defaults: RendererDefault[] = [];

  // #2265: one command step, `tr a-z A-Z`, and no named pipeline until one is saved.
  let steps: CommandStep[] = [{ id: "upper", name: "Upper", command: "tr a-z A-Z", timeout_secs: 10 }];
  let pipelines: NamedPipeline[] = [];
  // *Run pipelines on diffs* (#2270): on until set.
  let diffPipelinesOn = true;
  // `?fake-diff-pipeline` (#2270): `*.json.b64` opens, and diffs, through a saved pipeline.
  if (params.has("fake-diff-pipeline")) {
    pipelines = [{ id: "base64-json", name: "Base64 JSON", steps: [{ step: "base64_decode" }, { step: "pretty", format: "json" }] }];
    defaults = [{ pattern: "*.json.b64", renderer: "", pipeline: "base64-json" }];
  }
  const idOf = (name: string, i: number, kind: string) => name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || `${kind}-${i + 1}`;

  return {
    command_steps: () => steps,
    set_command_steps: (args) => {
      steps = (args["steps"] as CommandStepDraft[]).map((d, i) => ({
        id: d.id ?? idOf(d.name, i, "step"),
        name: d.name.trim(),
        command: d.command.trim(),
        timeout_secs: d.timeout_secs ?? 10,
      }));
      return steps;
    },
    named_pipelines: () => pipelines,
    set_named_pipelines: (args) => {
      pipelines = (args["pipelines"] as NamedPipelineDraft[]).map((d, i) => ({ id: d.id ?? idOf(d.name, i, "pipeline"), name: d.name.trim(), steps: d.steps }));
      return pipelines;
    },
    diff_pipelines: () => diffPipelinesOn,
    set_diff_pipelines: (args) => (diffPipelinesOn = args["on"] as boolean),
    renderer_defaults: () => defaults,
    set_renderer_defaults: (args) => {
      defaults = (args["defaults"] as RendererDefault[]).map((d) => ({ pattern: d.pattern.trim(), renderer: d.renderer.trim(), pipeline: d.pipeline?.trim() || null }));
      return defaults;
    },
    user_renderers: () => renderers,
    set_user_renderers: (args) => {
      const drafts = args["renderers"] as UserRendererDraft[];
      renderers = drafts.map((d, i) => ({
        id: d.id ?? (d.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || `renderer-${i + 1}`),
        name: d.name.trim(),
        command: d.command.trim(),
        patterns: d.patterns,
        fences: d.fences.map((f) => f.toLowerCase()),
        input: d.input,
        output: d.output,
        timeout_secs: Math.min(120, Math.max(1, d.timeout_secs ?? 10)),
        diff: d.diff,
      }));
      return renderers;
    },
    run_renderer: (args) => {
      const saved = renderers.find((r) => r.id === args["id"]);
      if (!saved) throw { code: "invalid", message: `'${String(args["id"])}' is not one of your renderers: see Settings › Renderers` };
      const content = args["content"] as RenderContent;
      if (saved.command !== "cat") return { outcome: "failed", status: 127, stderr: `the dev fake runs only cat, not ${saved.command}\n` };
      const body = content.kind === "text" ? content.text : atob(content.base64);
      return saved.output === "image" ? { outcome: "drawn", output: "image", body: `data:image/svg+xml;base64,${btoa(body)}` } : { outcome: "drawn", output: saved.output, body };
    },
    pages_kept_open: () => keptOpen,
    set_pages_kept_open: (args) => {
      keptOpen = Math.min(15, Math.max(0, Math.round(Number(args["count"]))));
      return keptOpen;
    },
    passive_attribution: () => passive,
    set_passive_attribution: (args) => {
      passive = args["enabled"] === true;
      return passive;
    },
    state_colours: () => stateColours(toneOverrides),
    set_tone_override: (args) => {
      toneOverrides.set(`${String(args["kind"])}\u0000${String(args["word"]).trim()}`, args["tone"] as ToneChoice);
      return stateColours(toneOverrides);
    },
    clear_tone_overrides: (args) => {
      const kind = String(args["kind"]);
      const word = args["word"];
      for (const key of [...toneOverrides.keys()]) {
        if (key === `${kind}\u0000${String(word)}` || (word === null && key.startsWith(`${kind}\u0000`))) toneOverrides.delete(key);
      }
      return stateColours(toneOverrides);
    },
    monitoring_settings: () => monitoring,
    set_monitoring_settings: (args) => {
      const next = args["settings"] as MonitoringSettings;
      if (!(next.sample_retention_days >= 1) || !(next.response_time_warn_ms >= 1)) {
        throw { code: "invalid", message: "both numbers must be at least 1", source_id: null };
      }
      monitoring = next;
      return monitoring;
    },
    backup_status: () => status(),
    backup_now: () => {
      const made = record(new Date(), 48 * MB + 100_000);
      archives.unshift(made);
      return made;
    },
    set_backup_schedule: (args) => {
      schedule = args["schedule"] as BackupSchedule;
      return status();
    },
    open_backup_folder: () => null,
  };
}
