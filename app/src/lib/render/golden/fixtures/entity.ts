import { invoke } from "@tauri-apps/api/core";

/** A source's item, as the mirror holds it. */
export interface Entity<T = unknown> {
  id: string;
  kind: "ticket" | "pr";
  fields: T;
}

export const RETRY = /retry-(\d+)/g;

export async function read(id: string, tries = 3): Promise<Entity | null> {
  for (let i = 0; i < tries; i += 1) {
    const got = await invoke<Entity>("get_entity", { id });
    if (got?.id) return { ...got, kind: got.kind ?? "ticket" };
  }
  return null; // gave up
}
