<!--
  The settings view — spec §14's "small settings surface", issue #69.

  ## Why this file exists, and how little of it there is

  §14 asks for export/schedule/restore and says "plain dialogs are fine", but
  knobas had no settings view at all to hang them off: `app/src/lib/` was
  `detail`, `ipc`, `launcher`, `shell`, `sources`. #69 was filed blocked on one
  existing; the ruling on that ticket (Fable, 2026-08-29, under delegation)
  found the blocker void — #36 closed without landing a settings view and no
  other open issue claims one — and gave this ticket the surface, at the
  smallest honest size:

  **Titled sections in one scrolling pane. No tabs, no router, no section
  registry.** More settings arrive in M2+ (share-export, monitoring), and the
  day the second one does it costs one component and one import — which is
  cheaper than the scaffolding that would have been built to receive it.

  The sources view stays where it is. Folding it in here is information
  architecture this ticket does not own.

  The second section arrived with #282 (passive attribution) and cost exactly
  what the ruling said it would: one component and one import. So did the third
  (#290, desktop notifications) and the fourth (#289, where standup protocols
  are published).
-->
<script lang="ts">
  import BackupSection from "./BackupSection.svelte";
  import NotificationsSection from "./NotificationsSection.svelte";
  import PassiveSection from "./PassiveSection.svelte";
  import StandupSection from "./StandupSection.svelte";
</script>

<div class="view">
  <div class="room-bar">
    <h1>Settings</h1>
  </div>

  <div class="view-b">
    <BackupSection />
    <PassiveSection />
    <NotificationsSection />
    <StandupSection />
  </div>
</div>
