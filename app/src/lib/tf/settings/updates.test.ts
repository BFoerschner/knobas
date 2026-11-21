/** Settings › Updates' words (ADR-0052, decisions D13): every row of the box's table, a state in, its strings out. */
import { afterAll, beforeAll, expect, test, vi } from "vitest";

import { fakeUpdate } from "../../shell/dev/fake/update";
import { bootLine, channelLine, cogTitle, megabytes, updateBox } from "./updates";

// Noon, so "today" and "yesterday" in the fixture's times are the same words whenever the suite runs.
const NOW = new Date(2026, 10, 21, 12, 0);
beforeAll(() => {
  vi.useFakeTimers({ now: NOW, toFake: ["Date"] });
});
afterAll(() => {
  vi.useRealTimers();
});

/** The table of decisions D13, by the fixture's named states. */
test("the box says each state's words and offers each state's buttons", () => {
  const rows: [string, string, RegExp, string | null, string | null][] = [
    // state, under the track, the line, the first button (null hidden), the second (null hidden)
    ["up-to-date", "Up to date", /^This is the newest version\.$/, null, null],
    ["checks-off", "Up to date on today", /^knobas doesn't check on its own\. Check now still does\.$/, null, null],
    ["available", "89 MB", /^knobas 3\.2\.9-ux28 is ready to download\. It changes the database, so knobas backs up your own data before it installs\.$/, null, "Download 89 MB"],
    ["downloading", "41 of 89 MB", /^Downloading from GitHub\. You can keep working\.$/, "Cancel", "Downloading…"],
    ["ready", "Downloaded and checked", /^Restarting closes knobas, backs up your own data, installs 3\.2\.9-ux28 and opens it again\. A sync that is running starts over afterwards\.$/, "Later", "Restart to update"],
    ["installing", "Restarting", /^Backing up your own data…$/, "Later", "Restarting…"],
    ["signature", "Not installed", /^The download did not carry knobas's signature, so it was deleted\. Nothing was installed and nothing was run\.$/, null, "Download again"],
    ["download-failed", "Stopped at 41 MB", /^The download stopped\. No bytes for 30s\. Nothing was installed\.$/, null, "Download again"],
    ["not-installed", "Not installed", /^3\.2\.9-ux28 did not install at \d\d:\d\d, so this is still 3\.2\.9-ux27\. Your data is as it was\. The download is kept\.$/, "Later", "Restart to update"],
    ["check-failed", "Up to date on today", /^knobas could not reach GitHub at \d\d:\d\d\. There was no connection\.$/, null, null],
    ["package", "63 MB", /^knobas 3\.2\.9 is out\. This copy came from a \.deb package, which knobas can't replace itself\. Download the new package and install it over this one\.$/, null, "Open the download page"],
    ["pre-on-releases", "Newer than the release", /^This pre-release build is newer than the newest release\. On Releases knobas stays on it until 3\.2\.10 or later is released\.$/, null, null],
    ["development", "", /^A development build doesn't update itself\.$/, null, null],
    ["not-set-up", "", /^Updates aren't set up for this build\.$/, null, null],
  ];
  for (const [name, reading, line, first, second] of rows) {
    const box = updateBox(fakeUpdate(name), NOW);
    expect(box.reading.text, name).toBe(reading);
    expect(box.says.line, name).toMatch(line);
    expect(box.first.shown ? box.first.label : null, name).toBe(first);
    expect(box.second.shown ? box.second.label : null, name).toBe(second);
  }
});

test("the second lines: when it was checked and when it checks again", () => {
  expect(updateBox(fakeUpdate("up-to-date"), NOW).says.dim).toMatch(/^Checked today \d\d:\d\d\. knobas checks again about \d\d:\d\d\.$/);
  expect(updateBox(fakeUpdate("available"), NOW).says.dim).toMatch(/^Checked today \d\d:\d\d\.$/);
  expect(updateBox(fakeUpdate("check-failed"), NOW).says.dim).toMatch(/^Last checked today \d\d:\d\d\. It tries again about \d\d:\d\d\.$/);
});

test("an update that changes no schema says so", () => {
  const state = fakeUpdate("available");
  state.offer = { ...state.offer!, changes_schema: false };
  expect(updateBox(state, NOW).says.line).toBe("knobas 3.2.9-ux28 is ready to download. It changes nothing in the database.");
});

test("the track: empty, an outline that points right, filling, whole", () => {
  expect(updateBox(fakeUpdate("up-to-date"), NOW).track).toEqual({ go: false, fill: null, bad: false });
  expect(updateBox(fakeUpdate("available"), NOW).track).toEqual({ go: true, fill: null, bad: false });
  expect(updateBox(fakeUpdate("downloading"), NOW).track.fill).toBeCloseTo(41 / 89);
  expect(updateBox(fakeUpdate("ready"), NOW).track).toEqual({ go: true, fill: 1, bad: false });
});

test("the two ends: this computer's version and the channel's newest", () => {
  const box = updateBox(fakeUpdate("available"), NOW);
  expect(box.here).toEqual({ label: "On this computer", version: "3.2.9-ux27", sub: "Pre-release build, since yesterday" });
  expect(box.there.label).toBe("Newest pre-release build");
  expect(box.there.version).toBe("3.2.9-ux28");
  expect(box.there.quiet).toBe(false);
  expect(box.there.sub).toMatch(/^Published today \d\d:\d\d$/);
  const package_ = updateBox(fakeUpdate("package"), NOW);
  expect(package_.here.sub).toBe("Release, from a .deb package");
  expect(package_.there.label).toBe("Newest release");
  expect(updateBox(fakeUpdate("up-to-date"), NOW).there.quiet).toBe(true);
});

test("the channel's line: one sentence on Releases, the warning on pre-release builds", () => {
  expect(channelLine("releases")).toEqual({ text: "Finished releases only.", warn: false });
  expect(channelLine("pre_releases").warn).toBe(true);
  expect(channelLine("pre_releases").text).toMatch(/^Pre-release builds can have bugs, .* until a newer release comes out\.$/);
});

test("the cog's title names a waiting update, and nothing without one", () => {
  expect(cogTitle("Settings (⌘,)", fakeUpdate("available"))).toBe("Settings (⌘,). knobas 3.2.9-ux28 is ready to download");
  expect(cogTitle("Settings (⌘,)", fakeUpdate("up-to-date"))).toBe("Settings (⌘,)");
  expect(cogTitle("Settings (⌘,)", null)).toBe("Settings (⌘,)");
});

test("megabytes as the box says them", () => {
  expect(megabytes(89_490_164)).toBe("89 MB");
  expect(megabytes(10)).toBe("1 MB");
});

test("the start screen's line: an offer and its press, or none and Check again", () => {
  expect(bootLine(fakeUpdate("available"))).toEqual({
    text: "knobas 3.2.9-ux28 is out and may fix this. The database is down, so no backup is taken first.",
    button: "Download and restart",
    does: "install",
  });
  expect(bootLine(fakeUpdate("up-to-date"))).toEqual({ text: "No newer version yet.", button: "Check again", does: "check" });
  expect(bootLine(fakeUpdate("not-set-up"))).toBeNull();
  expect(bootLine(fakeUpdate("package"))).toBeNull();
});
