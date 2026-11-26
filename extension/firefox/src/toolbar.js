// What the toolbar button shows for a router status: its icon, a badge while
// paused, and a title saying what a click does and why links stay in Firefox.

/**
 * @param {{paused: boolean, knobas: string, sources: number}} status
 * @returns {{icon: string, badge: string, title: string}}
 */
export function toolbarFor({ paused, knobas, sources }) {
  if (paused) {
    return { icon: "icons/off.svg", badge: "off", title: "knobas link handoff is paused: links open in Firefox. Click to resume." };
  }
  const why = {
    not_running: "knobas is not running, so links open in Firefox.",
    not_ready: "knobas is starting; links open in Firefox until it is ready.",
    no_host: "knobas's Firefox host is not installed: install it under a source's Open matching links in knobas in knobas's Settings.",
    incompatible: "this extension and knobas speak different protocol versions; update the older one.",
    error: "knobas did not answer; links open in Firefox.",
    unknown: "asking knobas for its sources…",
  }[knobas];
  const state =
    why ??
    (sources === 0
      ? "no source opens its links in knobas yet (Settings › a source › Open matching links in knobas)."
      : `links to ${sources === 1 ? "1 source" : `${sources} sources`} open in knobas.`);
  return { icon: "icons/on.svg", badge: "", title: `knobas link handoff: ${state} Click to pause.` };
}
