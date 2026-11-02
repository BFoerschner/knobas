/**
 * The relations a job's link offers, in its order (prototype-6's six): the
 * job's *Link to…* dialog (`TfLinkTo`) and the launcher's inline *Link to
 * ⟨job⟩* (#2426) pick from this one list, and both store the pick through
 * `drawn` (`detail/relations.ts`), so a link made either way is the same row.
 *
 * Not in `detail/relations.ts`: a crate reads that file (the estate's
 * `monitored-by` mirror test), and this list is the frontend's alone.
 */
export const JOB_LINK_RELATIONS: readonly { id: string; label: string }[] = [
  { id: "related", label: "related" },
  { id: "implements", label: "implements" },
  { id: "documents", label: "documents" },
  { id: "runs-on", label: "runs on" },
  { id: "blocks", label: "blocks" },
  { id: "depends-on", label: "depends on" },
];
