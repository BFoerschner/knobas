/**
 * The JAR view's reads (ux19) as the repository page hands them down: the bridge's `archive_*` commands and
 * `java_runtime`, a fake in tests. `open` names the archive by its repository, path and commit; the rest by the
 * handle an open answered.
 */

import { javaRuntime, type JavaRuntime } from "../ipc/app";
import { archiveClass, archiveDecompile, archiveDownload, archiveEntry, archiveNested, archiveOpen, type ArchiveIndex, type ClassOutline } from "../ipc/entity";

export interface ArchivePorts {
  open: (entityId: string, path: string, commit: string | null) => Promise<ArchiveIndex>;
  nested: (handle: string, entry: string) => Promise<ArchiveIndex>;
  entry: (handle: string, entry: string) => Promise<Uint8Array>;
  cls: (handle: string, entry: string) => Promise<ClassOutline>;
  decompile: (handle: string, entry: string) => Promise<string>;
  download: (handle: string, entry: string | null) => Promise<string>;
  java: () => Promise<JavaRuntime>;
}

export function bridgeArchive(): ArchivePorts {
  // Each looked up at its call, so a test that mocks a barrel without these never reaches them.
  return {
    open: (entityId, path, commit) => archiveOpen(entityId, path, commit),
    nested: (handle, entry) => archiveNested(handle, entry),
    entry: (handle, entry) => archiveEntry(handle, entry),
    cls: (handle, entry) => archiveClass(handle, entry),
    decompile: (handle, entry) => archiveDecompile(handle, entry),
    download: (handle, entry) => archiveDownload(handle, entry),
    java: () => javaRuntime(false),
  };
}
