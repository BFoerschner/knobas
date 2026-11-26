// One persistent Native Messaging port to knobas's host, opened on first use
// and again after it drops. The host serves frames until its stdin closes and
// echoes each request's `id`, so requests in flight are told apart by it.

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
  let port = null;
  let nextId = 1;
  /** @type {Map<number, {resolve: Function, reject: Function, timer: any}>} */
  const pending = new Map();

  function failAll(failure) {
    for (const { reject, timer } of pending.values()) {
      clearTimeout(timer);
      reject(failure);
    }
    pending.clear();
  }

  function open() {
    const opened = connect();
    opened.onMessage.addListener((message) => {
      const waiting = message && pending.get(message.id);
      if (!waiting) return;
      pending.delete(message.id);
      clearTimeout(waiting.timer);
      waiting.resolve(message);
    });
    opened.onDisconnect.addListener((dropped) => {
      if (port === opened) port = null;
      const reason = dropped?.error?.message ?? "the knobas host closed the connection";
      const code = /no such native application/i.test(reason) ? "no_host" : "disconnected";
      failAll(new NativeFailure(code, reason));
    });
    return opened;
  }

  return {
    /**
     * Sends `message` with a fresh `id` and resolves with the host's answer,
     * `ok: false` answers included. Rejects with a NativeFailure when there is none.
     */
    request(message, timeoutMs) {
      return new Promise((resolve, reject) => {
        const id = nextId++;
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new NativeFailure("timeout", `no answer from knobas within ${timeoutMs} ms`));
        }, timeoutMs);
        pending.set(id, { resolve, reject, timer });
        try {
          port ??= open();
          port.postMessage({ ...message, id });
        } catch (cause) {
          port = null;
          pending.delete(id);
          clearTimeout(timer);
          reject(new NativeFailure("disconnected", String(cause?.message ?? cause)));
        }
      });
    },
  };
}
