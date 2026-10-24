/**
 * What an external adapter's install and update say before the reader
 * confirms (#2031, spec #1192 story 21): the roles it claims, every host it
 * may reach and the limits it asks for, and what an update changes in them.
 *
 * Pure: the dialogs hand in what `preview_install_adapter` and
 * `preview_update_adapter` answered, and draw the strings that come back.
 */
import type { AdapterLimits, AdapterPreview, RoleClaim } from "../../ipc/sources";
import { secondsLabel } from "../../time/format";
import { roleWord } from "./settings";

/** One manifest host in words: a fixed host as written, a template by the setting it is read from. */
function hostWord(host: string): string {
  const key = /^\{(.+)\}$/.exec(host)?.[1];
  if (key === undefined) return host;
  return key === "base_url" ? "this source's address" : `the host in its ${key} setting`;
}

/** `a`, `a and b`, `a, b and c`. */
function listed(words: string[]): string {
  return words.length <= 1 ? (words[0] ?? "") : `${words.slice(0, -1).join(", ")} and ${words.at(-1)}`;
}

/** The manifest's hosts as a sentence: what the adapter may reach, and only that. */
export function reach(hosts: string[]): string {
  if (hosts.length === 0) return "It may reach no host.";
  return `It may reach ${listed(hosts.map(hostWord))}, and no other host.`;
}

/** The roles a descriptor claims, in the shell's words; a custom role by its name. */
export function rolesLine(roles: RoleClaim[]): string {
  return roles.length === 0 ? "None" : roles.map((claim) => roleWord(claim.role)).join(", ");
}

/** The limits every call is held to, each said as asked for or the default. */
export function limitsLine(limits: AdapterLimits): string {
  const deadline = `${secondsLabel(limits.deadline_secs)} per call (${limits.deadline_asked ? "asked for" : "the default"})`;
  const memory = `${limits.memory_mib} MiB of memory (${limits.memory_asked ? "asked for" : "the default"})`;
  return `${deadline}, ${memory}`;
}

/**
 * What an update changes in what the adapter may do: the hosts it newly
 * reaches or no longer reaches, the roles it newly claims or drops, and a
 * limit that moves. Empty when the release asks for what is installed.
 */
export function updateChanges(installed: AdapterPreview, release: AdapterPreview): string[] {
  const lines: string[] = [];
  const gained = (now: string[], next: string[]) => next.filter((entry) => !now.includes(entry));
  const hostsIn = gained(installed.hosts, release.hosts);
  const hostsOut = gained(release.hosts, installed.hosts);
  if (hostsIn.length > 0) lines.push(`It may newly reach ${listed(hostsIn.map(hostWord))}.`);
  if (hostsOut.length > 0) lines.push(`It no longer reaches ${listed(hostsOut.map(hostWord))}.`);
  const roles = (claims: RoleClaim[]) => claims.map((claim) => claim.role);
  const rolesIn = gained(roles(installed.roles), roles(release.roles));
  const rolesOut = gained(roles(release.roles), roles(installed.roles));
  if (rolesIn.length > 0) lines.push(`It newly claims ${listed(rolesIn.map(roleWord))}.`);
  if (rolesOut.length > 0) lines.push(`It no longer claims ${listed(rolesOut.map(roleWord))}.`);
  const was = installed.limits;
  const next = release.limits;
  if (was.deadline_secs !== next.deadline_secs) {
    lines.push(`Each call may take ${secondsLabel(next.deadline_secs)}, was ${secondsLabel(was.deadline_secs)}.`);
  }
  if (was.memory_mib !== next.memory_mib) {
    lines.push(`Each call may use ${next.memory_mib} MiB of memory, was ${was.memory_mib} MiB.`);
  }
  return lines;
}
