/**
 * Whether the *Add a source* dialog is up, and how many sources it has added
 * (#830).
 *
 * The dialog is the rooms UI's `AddSource.svelte`, unported, so the shell
 * mounts it outside `.tf` where it reads its own tokens (#809's ruling on
 * rooms-styled dialogs). Settings opens it through this state and re-reads
 * its list when `added` moves.
 */
export const addSourcePanel = $state({ open: false, added: 0 });
