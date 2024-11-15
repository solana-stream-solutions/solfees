import { CustomRow, CustomRowBasic } from "./prepareValidatorRow.ts";

// TODO probably we should change CommitmentStatus to Enum and compare as if(elt.commitment in CommitmentStatus) to find extended statuses
export function isReal(
  slot: CustomRow["slots"][number] | CustomRowBasic["slots"][number]
): slot is CustomRowBasic["slots"][number] {
  return slot.commitment !== "next-leader" && slot.commitment !== "scheduled";
}
