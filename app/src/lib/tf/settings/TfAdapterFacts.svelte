<!--
  What an external adapter's release is and may do (#2031), as the install
  and the update dialogs show it before the reader confirms: its version and
  release, where it came from, the roles it claims, every host it may reach
  and the limits it asks for. The words are `adapter-preview.ts`'s.
-->
<script lang="ts">
  import type { AdapterPreview } from "../../ipc/sources";
  import { limitsLine, reach, rolesLine } from "./adapter-preview";

  let { preview }: { preview: AdapterPreview } = $props();
</script>

<dl class="tf-facts">
  <dt>Version</dt>
  <dd>{preview.adapter_version} · {preview.tag ?? "no release"}</dd>
  <dt>From</dt>
  <dd class="tf-ext-from">{preview.source_url}</dd>
  <dt>Roles</dt>
  <dd>{rolesLine(preview.roles)}</dd>
  <dt>Hosts</dt>
  <dd>{reach(preview.hosts)}</dd>
  <dt>Limits</dt>
  <dd>{limitsLine(preview.limits)}</dd>
</dl>
