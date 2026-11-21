/**
 * Settings › Updates' words (ADR-0052, decisions D13): what the box of two versions says in each state, pure over the
 * updater's state, so every sentence is a tested string as in `settings.ts`.
 *
 * The box has the same rooms in every state: two ends of three lines, the track and its one reading, two lines of what it
 * says, and two buttons. Only the words, the track's fill and the buttons' words change. The first button means "not now"
 * in every state, the second is the step forward.
 */
import type { LastUpdate, Phase, UpdateState } from "../../ipc/update";
import { dayHeadingLabel, stampLabel } from "../../time/format";

/** What the second button does when pressed. */
export type Forward = "download" | "install" | "page" | null;

/** One of the box's two buttons. */
export interface BoxButton {
  label: string;
  /** Drawn with `visibility: hidden` when false, so its room stays. */
  shown: boolean;
  disabled: boolean;
}

/** Everything the box draws. */
export interface UpdateBox {
  here: { label: string; version: string; sub: string };
  there: { label: string; version: string; quiet: boolean; sub: string };
  /** `go`: there is somewhere to go; `fill`: 0 to 1, `null` for none; `bad`: the fill is a failure's. */
  track: { go: boolean; fill: number | null; bad: boolean };
  reading: { text: string; tone: "ok" | "err" | null };
  says: { line: string; dim: string | null; err: boolean };
  first: BoxButton & { does: "cancel" | "later" | null };
  second: BoxButton & { does: Forward };
  /** *Open the download page*, left of the buttons, where the second is not already it. */
  page: boolean;
}

/** The longest words each button ever holds: their rooms are measured on these. */
export const FIRST_WORDS = ["Cancel", "Later"] as const;
export const SECOND_WORDS = ["Download 400 MB", "Downloading…", "Restart to update", "Restarting…", "Download again", "Open the download page"] as const;

/** The two words under *Which versions*. */
export const CHANNEL_NAMES = { releases: "Releases", pre_releases: "Pre-release builds" } as const;

/** The line under *Which versions*: one room for either. */
export function channelLine(channel: UpdateState["channel"]): { text: string; warn: boolean } {
  return channel === "releases"
    ? { text: "Finished releases only.", warn: false }
    : {
        text: "Pre-release builds can have bugs, and they can change the database in a way an older knobas can't open. knobas backs up your own data before each update. Back on Releases you keep the build you have until a newer release comes out.",
        warn: true,
      };
}

/** *Last update*'s line. */
export function lastUpdateLine(last: LastUpdate | null, now: Date): { from: string; to: string; when: string; backup: string | null } | null {
  if (last === null || !last.installed) return null;
  return { from: last.from, to: last.to, when: stampLabel(last.at, now), backup: last.backup };
}

/** Megabytes as the box says them: `89 MB`, `41 of 89 MB`. */
export function megabytes(bytes: number): string {
  return `${Math.max(1, Math.round(bytes / 1_000_000))} MB`;
}

/** The words for one standing's copy kind. */
function packageWord(state: UpdateState): string {
  return state.standing.standing === "by_hand" && state.standing.kind === "rpm" ? ".rpm" : ".deb";
}

/** Whether the cog and the row carry the dot. */
export function marked(state: UpdateState | null): boolean {
  return state !== null && state.marked;
}

/** The cog's title: `Settings (⌘,)`, and the update when one is waiting. */
export function cogTitle(base: string, state: UpdateState | null): string {
  if (state === null || !state.marked) return base;
  if (state.last !== null && !state.last.installed && state.offer?.version === state.last.to) return `${base}. knobas ${state.last.to} did not install`;
  return state.offer === null ? base : `${base}. knobas ${state.offer.version} is ready to download`;
}

/** The toast after an update went in. */
export function updatedToast(state: UpdateState): string {
  return `knobas is now ${state.current}`;
}

const channelNewest = (state: UpdateState) => (state.channel === "releases" ? "Newest release" : "Newest pre-release build");

/** The box, for `state` at `now`. */
export function updateBox(state: UpdateState, now: Date): UpdateBox {
  const checked = state.checked_at === null ? null : stampLabel(state.checked_at, now);
  const nextAt = state.next_check_at === null ? null : stampLabel(state.next_check_at, now, true);
  const since = state.last !== null && state.last.installed && state.last.to === state.current ? `, since ${dayOf(state.last.at, now)}` : "";
  const kind = state.standing.standing === "by_hand" && state.standing.why === "package" ? `, from a ${packageWord(state)} package` : since;
  const here = {
    label: "On this computer",
    version: state.current,
    sub: `${state.current_is_pre_release ? "Pre-release build" : "Release"}${kind}`,
  };
  const newest = state.newest;
  const offer = state.offer;
  const there = {
    label: channelNewest(state),
    version: newest?.version ?? "",
    quiet: offer === null,
    sub: newest?.published_at ? `Published ${stampLabel(newest.published_at, now)}` : "",
  };
  const hidden = (label: string): BoxButton => ({ label, shown: false, disabled: true });
  type Buttons = Pick<UpdateBox, "first" | "second" | "page">;
  const none: Buttons = {
    first: { ...hidden("Later"), does: null },
    second: { ...hidden("Download"), does: null },
    page: false,
  };
  const size = offer?.bytes ? megabytes(offer.bytes) : "";
  const checkedLine = checked === null ? null : `Checked ${checked}.`;
  const box = (track: UpdateBox["track"], reading: UpdateBox["reading"], says: UpdateBox["says"], buttons: Buttons = none): UpdateBox => ({
    here,
    there,
    track,
    reading,
    says,
    ...buttons,
  });
  const phase: Phase = state.phase;

  if (!state.set_up) {
    return box({ go: false, fill: null, bad: false }, { text: "", tone: null }, { line: "Updates aren't set up for this build.", dim: null, err: false });
  }
  if (state.standing.standing === "by_hand" && state.standing.why === "development") {
    return box({ go: false, fill: null, bad: false }, { text: "", tone: null }, { line: "A development build doesn't update itself.", dim: null, err: false });
  }

  // An install under way: each step said as it runs, both buttons waiting.
  if (phase.phase === "installing") {
    const step = { checking: "Checking the download…", backing_up: "Backing up your own data…", replacing: "Putting the new knobas in place…", closing: "Closing…" }[phase.step];
    const then = state.standing.standing === "in_place" && (state.standing.kind === "nsis" || state.standing.kind === "msi") ? "Then knobas closes and the installer opens it again." : "Then knobas opens again.";
    return box({ go: true, fill: 1, bad: false }, { text: "Restarting", tone: null }, { line: step, dim: phase.step === "closing" ? null : then, err: false }, {
      first: { label: "Later", shown: true, disabled: true, does: null },
      second: { label: "Restarting…", shown: true, disabled: true, does: null },
      page: false,
    });
  }
  if (phase.phase === "downloading") {
    const total = phase.total ?? offer?.bytes ?? null;
    const fill = total ? Math.min(1, phase.received / total) : 0;
    const reading = total ? `${megabytes(phase.received).replace(" MB", "")} of ${megabytes(total)}` : megabytes(phase.received);
    return box({ go: true, fill, bad: false }, { text: reading, tone: null }, { line: "Downloading from GitHub. You can keep working.", dim: null, err: false }, {
      first: { label: "Cancel", shown: true, disabled: false, does: "cancel" },
      second: { label: "Downloading…", shown: true, disabled: true, does: null },
      page: false,
    });
  }
  if (phase.phase === "ready" && offer !== null) {
    const notInstalled = state.last !== null && !state.last.installed && state.last.to === offer.version;
    const buttons: Buttons = {
      first: { label: "Later", shown: true, disabled: false, does: "later" },
      second: { label: "Restart to update", shown: true, disabled: false, does: "install" as Forward },
      page: notInstalled,
    };
    if (notInstalled) {
      const at = stampLabel(state.last!.at, now, true);
      return box({ go: true, fill: 1, bad: false }, { text: "Not installed", tone: "err" }, { line: `${offer.version} did not install at ${at}, so this is still ${state.current}. Your data is as it was. The download is kept.`, dim: null, err: true }, buttons);
    }
    return box({ go: true, fill: 1, bad: false }, { text: "Downloaded and checked", tone: "ok" }, { line: `Restarting closes knobas, backs up your own data, installs ${offer.version} and opens it again. A sync that is running starts over afterwards.`, dim: null, err: false }, buttons);
  }
  if (phase.phase === "failed" && offer !== null) {
    const again: Buttons = { first: { ...hidden("Later"), does: null }, second: { label: "Download again", shown: true, disabled: false, does: "download" as Forward }, page: false };
    if (phase.stage === "signature") {
      return box({ go: true, fill: null, bad: true }, { text: "Not installed", tone: "err" }, { line: "The download did not carry knobas's signature, so it was deleted. Nothing was installed and nothing was run.", dim: null, err: true }, again);
    }
    if (phase.stage === "download") {
      const at = phase.received ? `Stopped at ${megabytes(phase.received)}` : "Stopped";
      return box({ go: true, fill: null, bad: true }, { text: at, tone: "err" }, { line: `The download stopped. ${sentence(phase.message)} Nothing was installed.`, dim: null, err: true }, again);
    }
    const restart: Buttons = { first: { label: "Later", shown: true, disabled: false, does: "later" as const }, second: { label: "Open the download page", shown: true, disabled: false, does: "page" as Forward }, page: false };
    if (phase.stage === "backup") {
      return box({ go: true, fill: 1, bad: false }, { text: "Not installed", tone: "err" }, { line: `${sentence(phase.message)} Nothing was closed.`, dim: null, err: true }, {
        ...restart,
        second: { label: "Restart to update", shown: true, disabled: false, does: "install" },
      });
    }
    return box({ go: true, fill: null, bad: true }, { text: "Not installed", tone: "err" }, { line: `${sentence(phase.message)} The installed knobas is as it was.`, dim: null, err: true }, restart);
  }

  // A check that failed: what was known stays.
  if (state.check_failed !== null) {
    const failed = state.check_failed;
    const at = stampLabel(failed.at, now, true);
    const why = failed.kind === "offline" ? "There was no connection." : failed.kind === "rate_limited" ? "GitHub's limit of requests for this address is used up for now." : sentence(failed.message);
    const reading = offer !== null ? size : newest !== null ? `Up to date on ${dayOf(failed.at, now)}` : "";
    const dim = `Last checked ${checked ?? "never"}.${nextAt === null ? "" : ` It tries again about ${nextAt}.`}`;
    return box({ go: offer !== null, fill: null, bad: false }, { text: reading, tone: null }, { line: `knobas could not reach GitHub at ${at}. ${why}`, dim, err: false }, offer === null ? none : forward(state, size));
  }

  if (offer !== null) {
    if (state.standing.standing === "by_hand") {
      const line =
        state.standing.why === "translocated"
          ? `knobas ${offer.version} is out. Move knobas into Applications first, then it updates itself.`
          : `knobas ${offer.version} is out. This copy came from a ${packageWord(state)} package, which knobas can't replace itself. Download the new package and install it over this one.`;
      return box({ go: true, fill: null, bad: false }, { text: size, tone: null }, { line, dim: null, err: false }, forward(state, size));
    }
    const schema = offer.changes_schema === false ? "It changes nothing in the database." : "It changes the database, so knobas backs up your own data before it installs.";
    return box({ go: true, fill: null, bad: false }, { text: size, tone: null }, { line: `knobas ${offer.version} is ready to download. ${schema}`, dim: checkedLine, err: false }, forward(state, size));
  }

  if (newest !== null && state.current_is_pre_release && state.channel === "releases" && newest.version !== state.current) {
    const next = state.current.split("-")[0]!;
    return box({ go: false, fill: null, bad: false }, { text: "Newer than the release", tone: null }, { line: `This pre-release build is newer than the newest release. On Releases knobas stays on it until ${next} or later is released.`, dim: checkedLine, err: false });
  }

  if (!state.check) {
    const day = state.checked_at === null ? "" : dayOf(state.checked_at, now);
    return box({ go: false, fill: null, bad: false }, { text: day ? `Up to date on ${day}` : "", tone: null }, { line: "knobas doesn't check on its own. Check now still does.", dim: checkedLine, err: false });
  }

  if (state.checked_at === null) {
    const dim = nextAt === null ? null : `knobas checks about ${nextAt}.`;
    return box({ go: false, fill: null, bad: false }, { text: "", tone: null }, { line: "knobas has not checked yet.", dim, err: false });
  }

  const newestWord = state.channel === "releases" ? "This is the newest release." : "This is the newest version.";
  const dim = `Checked ${checked}.${nextAt === null ? "" : ` knobas checks again about ${nextAt}.`}`;
  return box({ go: false, fill: null, bad: false }, { text: "Up to date", tone: "ok" }, { line: newestWord, dim, err: false });
}

/** The forward step when a version is on offer: download it, or open its page where this copy is updated by hand. */
function forward(state: UpdateState, size: string): Pick<UpdateBox, "first" | "second" | "page"> {
  const byHand = state.standing.standing === "by_hand";
  return {
    first: { label: "Later", shown: false, disabled: true, does: null },
    second: byHand
      ? { label: "Open the download page", shown: true, disabled: false, does: "page" }
      : { label: size ? `Download ${size}` : "Download", shown: true, disabled: false, does: "download" },
    page: false,
  };
}

/** `Today`, `yesterday`'s or a date, for *since* and *Up to date on*. */
function dayOf(iso: string, now: Date): string {
  const words = dayHeadingLabel(new Date(iso), now);
  return words === "Today" ? "today" : words === "Yesterday" ? "yesterday" : words.replace(/^\w+ /, "");
}

/** A message as a sentence: a capital, a full stop. */
function sentence(message: string): string {
  const text = message.trim();
  if (text === "") return "";
  const capital = text[0]!.toUpperCase() + text.slice(1);
  return /[.!?]$/.test(capital) ? capital : `${capital}.`;
}

/** The start screen's line (decisions D14): an offer and its button, or none and *Check again*. */
export function bootLine(state: UpdateState | null): { text: string; button: string; does: "install" | "check" } | null {
  if (state === null || !state.set_up || state.standing.standing !== "in_place") return null;
  if (state.offer === null) return { text: "No newer version yet.", button: "Check again", does: "check" };
  const backup = state.offer.changes_schema === false ? "" : " The database is down, so no backup is taken first.";
  return { text: `knobas ${state.offer.version} is out and may fix this.${backup}`, button: "Download and restart", does: "install" };
}

/** A step of the start screen's one-press install, said in its line. */
export function bootStep(phase: Phase): string | null {
  if (phase.phase === "downloading") return phase.total ? `Downloading ${megabytes(phase.received).replace(" MB", "")} of ${megabytes(phase.total)}…` : "Downloading…";
  if (phase.phase === "ready") return "Downloaded and checked.";
  if (phase.phase === "installing") return { checking: "Checking the download…", backing_up: "Backing up…", replacing: "Putting the new knobas in place…", closing: "Closing…" }[phase.step];
  if (phase.phase === "failed") return `Not installed: ${phase.message}`;
  return null;
}
