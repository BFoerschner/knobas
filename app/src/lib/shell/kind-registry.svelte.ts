/**
 * What the installed adapters declare about their kinds.
 *
 * §3a: *"an adapter's descriptor declares everything the app needs to host
 * it"*, and *"entity kinds with display metadata drive grouping, chips and
 * labels"*. This is the layer that makes that literally true for the shell —
 * the only one that can be right about a kind knobas has never seen — and it
 * is why `kinds.ts` has three layers with the adapter's on top.
 *
 * ## Asked once
 *
 * `list_adapters` is static per build (the descriptor is a `fn() ->
 * SourceDescriptor` over a compiled-in table) and answers *before* the
 * database is up, touching no keychain. So it is read once at shell start and
 * cached — with one exception: a **failed** read is not cached, because
 * `invoke` rejects on the first frames before Tauri has injected its
 * internals, and caching that would leave the whole session with no kind
 * metadata.
 *
 * ## Two lookups, and why there are two
 *
 * A room groups by kind across every source, so it has a kind and nothing
 * else: {@link KindRegistry.info}. A detail header knows which source the
 * entity came from, and a `ticket` from two adapters is two different words —
 * {@link KindRegistry.infoFor} is what keeps it from being labelled with the
 * other adapter's. The Rust side resolves `EntityDetail.kind_info` the same
 * way, by adapter and not by kind, for the same reason.
 */
import { listAdapters as realListAdapters } from "../ipc/sources";
import type { KindInfo } from "../ipc/entity";
import type { SourceDescriptor } from "../ipc/sources";

/** The one command this needs, injectable so a test needs no Tauri bridge. */
export interface KindRegistryPorts {
  listAdapters: () => Promise<SourceDescriptor[]>;
}

export interface KindRegistry {
  /**
   * What any adapter declares about this kind, first declaration winning.
   *
   * "First" is a stable answer, not a race: `list_adapters` answers from a
   * fixed table in a fixed order.
   */
  info(kind: string): KindInfo | null;
  /** What *this* adapter declares about this kind. */
  infoFor(adapterKind: string, kind: string): KindInfo | null;
  /**
   * The write-back ops this adapter declares (`SourceDescriptor.write_ops`),
   * by their `knobas_source::WriteOp` identifiers — `"transition"`,
   * `"comment"`, and so on.
   *
   * What an affordance that queues a write is drawn from (#179):
   * `submit_write` rejects an op the source does not offer, and the surface
   * that offered it is the thing at fault. An adapter the registry has not
   * heard of declares nothing, so nothing is offered — the same direction
   * every other read here fails in.
   */
  writeOps(adapterKind: string): string[];
  /** Read the registry. Idempotent after a success; retryable after a failure. */
  load(): Promise<void>;
}

export function createKindRegistry(ports?: KindRegistryPorts): KindRegistry {
  const io = ports ?? { listAdapters: realListAdapters };

  const state = $state<{ adapters: SourceDescriptor[] }>({ adapters: [] });
  let loaded = false;
  /** The in-flight read, so three components mounting at once ask once. */
  let inFlight: Promise<void> | null = null;

  return {
    info(kind: string) {
      for (const adapter of state.adapters) {
        const found = adapter.entity_kinds.find((info) => info.id === kind);
        if (found) return found;
      }
      return null;
    },
    infoFor(adapterKind: string, kind: string) {
      const adapter = state.adapters.find((entry) => entry.adapter_kind === adapterKind);
      return adapter?.entity_kinds.find((info) => info.id === kind) ?? null;
    },
    writeOps(adapterKind: string) {
      const adapter = state.adapters.find((entry) => entry.adapter_kind === adapterKind);
      return adapter?.write_ops ?? [];
    },
    load() {
      if (loaded) return Promise.resolve();
      if (inFlight) return inFlight;
      inFlight = io
        .listAdapters()
        .then((adapters) => {
          state.adapters = adapters;
          loaded = true;
        })
        .catch(() => {
          // Not cached: every caller's fallback is §3a's humaniser, so the
          // shell still draws, and the next mount gets another go.
        })
        .finally(() => {
          inFlight = null;
        });
      return inFlight;
    },
  };
}

/**
 * The one the window uses.
 *
 * Module-level, like `toasts` and `health`, because the room, the tiles, the
 * detail header and the launcher rows all read it and threading a context to
 * each would be ceremony around a table that never changes.
 */
export const kindRegistry = createKindRegistry();
