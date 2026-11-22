<!--
  One of an entry's attachments in the app's file viewer (step 9a, ADR-0056): read live when it opens, one request,
  and held by this dialog alone; closing it drops the bytes. Text draws as text, an image as itself, a PDF in the
  viewer's PDF component, anything else as its kind with *Download*, which saves it to the Downloads folder.
-->
<script lang="ts">
  import TfFileViewer, { type ViewerContent } from "../render/TfFileViewer.svelte";
  import type { Layers } from "../layers.svelte";
  import type { AttachmentAnswer } from "../../ipc/passwords";
  import { attachmentKind } from "../job/attachments";
  import { formatBytes } from "../../sources/diagnostics";
  import { ipcErrorMessage } from "../../ipc";
  import type { PasswordPorts } from "./ports";

  let {
    entityId,
    file,
    word,
    entryTitle,
    ports,
    layers,
    onsave,
    onclose,
  }: {
    entityId: string;
    file: { id: string; name: string; size: number | null };
    /** The source in the page's words: *the password manager*, or its name. */
    word: string;
    entryTitle: string;
    ports: PasswordPorts;
    layers?: Layers | undefined;
    onsave: () => void;
    onclose: () => void;
  } = $props();

  let answer = $state<AttachmentAnswer | null>(null);
  let failed = $state<string | null>(null);
  /** Bumped on close: a read that lands after it is dropped. */
  let generation = 0;

  async function read(): Promise<void> {
    const mine = ++generation;
    failed = null;
    answer = null;
    try {
      const next = await ports.attachment(entityId, file.id);
      if (mine === generation) answer = next;
    } catch (cause) {
      if (mine === generation) failed = ipcErrorMessage(cause);
    }
  }

  $effect(() => {
    void read();
    return () => {
      generation++;
      answer = null;
    };
  });

  const IMAGE = /\.(png|jpe?g|gif|webp)$/i;
  const bytes = (data: string): Uint8Array => Uint8Array.from(atob(data), (c) => c.charCodeAt(0));

  const content = $derived.by((): ViewerContent => {
    if (failed !== null) return { kind: "missing", why: failed, retry: () => void read() };
    if (answer === null) return { kind: "reading" };
    switch (answer.kind) {
      case "unreachable":
        return { kind: "missing", why: `${word.charAt(0).toUpperCase()}${word.slice(1)} does not answer, so ${file.name} was not read.`, retry: () => void read() };
      case "not_offered":
        return { kind: "missing", why: `${file.name} is no longer listed on this entry. Sync now to see what it holds.`, retry: null };
      case "too_large":
        return { kind: "missing", why: `${file.name} is ${formatBytes(answer.size)}, more than the viewer reads. Download saves it.`, retry: null };
      case "read": {
        if (answer.text !== null) return { kind: "text", text: answer.text, truncated: false };
        if (IMAGE.test(file.name)) return { kind: "image", src: `data:image/${file.name.split(".").pop()!.toLowerCase().replace("jpg", "jpeg")};base64,${answer.data}` };
        if (/\.pdf$/i.test(file.name)) {
          const data = answer.data;
          return { kind: "pdf", read: () => Promise.resolve(bytes(data)), size: answer.size };
        }
        return { kind: "none" };
      }
    }
  });

  const line = $derived([file.size === null ? null : formatBytes(file.size), `attached to ${entryTitle}`, "read just now and held in this window; closing drops it"].filter((p) => p !== null).join(", "));
</script>

<TfFileViewer name={file.name} {line} kind={attachmentKind({ name: file.name, mime: null })} {content} download={onsave} {...layers ? { layers } : {}} {onclose} />
