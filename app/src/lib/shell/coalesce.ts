/**
 * A re-read that an event storm runs at most twice (#753).
 *
 * The inbox and the open alerts re-read on every `activity:new` and every
 * `sync:state`, and a *Sync all* over five sources is a burst of both. Each
 * event used to start a read of its own -- the inbox's is three statements on
 * the five-connection UI pool -- so a burst put as many reads in flight as it
 * had events, every one of them answering the same question, while the
 * reader's own clicks waited for a connection.
 *
 * Coalesced: while a read is in flight, any number of further events make
 * **one** more read, started when it lands. Not zero: an event during a read
 * may be news that read began too early to see, so the last word is always a
 * read that started after the last event.
 */
export function coalesce(read: () => Promise<void>): () => Promise<void> {
  let running: Promise<void> | null = null;
  let again: Promise<void> | null = null;
  const start = (): Promise<void> => {
    const current = read().finally(() => {
      if (running === current) running = null;
    });
    running = current;
    return current;
  };
  return () => {
    if (running === null) return start();
    if (again === null) {
      again = running
        .catch(() => {})
        .then(() => {
          again = null;
          return start();
        });
    }
    return again;
  };
}
