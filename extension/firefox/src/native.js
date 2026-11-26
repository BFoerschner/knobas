// Native Messaging ports to knobas's host, each carrying one request at a
// time. The host serves a port's frames one after another and stamps each
// request when it gets to it, and knobas acts on a request only within 1.5 s
// of that stamp. A request queued behind a slow one on a shared port would be
// stamped late, and knobas could open it after this extension had given up
// and let Firefox load the page: the link would open twice. So a request goes
// to an idle port (a new one when none is idle), and a port whose request
// timed out is closed, with whatever it still holds. Normally one port serves
// everything: requests rarely overlap.

/** Why a request got no answer: `no_host` (no manifest), `disconnected` or `timeout`. */
export class NativeFailure extends Error {
  /** @param {"no_host" | "disconnected" | "timeout"} code */
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

/**
 * @param {() => object} connect opens a port, `browser.runtime.connectNative(HOST_NAME)`
 */
export function createNativeClient(connect) {
  let nextId = 1;
  /** The open port with no request in flight, if any (at most one). */
  const idle = [];

  function forget(line) {
    line.dead = true;
    const at = idle.indexOf(line);
    if (at >= 0) idle.splice(at, 1);
  }

  /** Closes a port Firefox still holds open; Firefox then ends its host process. */
  function close(line) {
    forget(line);
    try {
      line.port.disconnect();
    } catch {
      // Already gone.
    }
  }

  function open() {
    const line = { port: connect(), waiting: null, dead: false };
    line.port.onMessage.addListener((message) => {
      const waiting = line.waiting;
      if (!waiting || message?.id !== waiting.id) return;
      line.waiting = null;
      clearTimeout(waiting.timer);
      // One idle port is enough; a second one, left from requests that
      // overlapped, would keep a host process running for nothing.
      if (idle.length > 0) close(line);
      else if (!line.dead) idle.push(line);
      waiting.resolve(message);
    });
    line.port.onDisconnect.addListener((dropped) => {
      forget(line);
      const waiting = line.waiting;
      line.waiting = null;
      if (!waiting) return;
      clearTimeout(waiting.timer);
      const reason = dropped?.error?.message ?? "the knobas host closed the connection";
      const code = /no such native application/i.test(reason) ? "no_host" : "disconnected";
      waiting.reject(new NativeFailure(code, reason));
    });
    return line;
  }

  return {
    /**
     * Sends `message` with a fresh `id` on an idle port and resolves with the
     * host's answer, `ok: false` answers included. Rejects with a
     * NativeFailure when there is none.
     */
    request(message, timeoutMs) {
      return new Promise((resolve, reject) => {
        const id = nextId++;
        let line = null;
        try {
          line = idle.pop() ?? open();
          const timer = setTimeout(() => {
            line.waiting = null;
            close(line);
            reject(new NativeFailure("timeout", `no answer from knobas within ${timeoutMs} ms`));
          }, timeoutMs);
          line.waiting = { id, resolve, reject, timer };
          line.port.postMessage({ ...message, id });
        } catch (cause) {
          if (line) {
            clearTimeout(line.waiting?.timer);
            line.waiting = null;
            close(line);
          }
          reject(new NativeFailure("disconnected", String(cause?.message ?? cause)));
        }
      });
    },
  };
}
