/**
 * What the Code page reads a generated folder through (ADR-0046): its tree and its files by the token the run answered.
 * The IPC, a fake in tests.
 */
import { folderRenderArchiveOpen, folderRenderFile, folderRenderFileBytes, folderRenderTree, type MadeTree } from "../ipc/app";
import type { ArchiveIndex, ViewBlock } from "../ipc/generated/entity";

export interface MadePorts {
  tree(token: string): Promise<MadeTree>;
  /** A generated file as a checkout's file reads, `path` from the generated folder's top. */
  file(token: string, path: string): Promise<ViewBlock>;
  fileBytes(token: string, path: string): Promise<Uint8Array>;
  /** A generated archive listed for the JAR view; the view's other reads name it by the handle answered. Absent: no JAR view. */
  archive?(token: string, path: string): Promise<ArchiveIndex>;
}

export const ipcMadePorts: MadePorts = {
  tree: (token) => folderRenderTree(token),
  file: (token, path) => folderRenderFile(token, path),
  fileBytes: (token, path) => folderRenderFileBytes(token, path),
  archive: (token, path) => folderRenderArchiveOpen(token, path),
};
