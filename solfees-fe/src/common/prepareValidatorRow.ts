import { CommitmentStatus, SlotContent } from "../store/websocketStore.ts";

export type ExtendedCommitmentStatus = CommitmentStatus | "next-leader" | "scheduled";
type ExtendedSlotContent = Omit<SlotContent, "commitment"> & {
  commitment: ExtendedCommitmentStatus;
};

export type CustomRowBasic = {
  id: string;
  leader: string;
  slots: {
    commitment: ExtendedCommitmentStatus;
    slot: number;
  }[];
  transactions: {
    filtered: number;
    votedDiff: number;
    total: number;
  }[];
  computeUnits: {
    amount: number;
    percent: number;
  }[];
  earnedSol: number[];
  averageFee: number[];
  fee0: number[];
  fee1: number[];
  fee2: number[];
};
export type CustomRow = Omit<CustomRowBasic, "slots"> & {
  slots: { slot: number; commitment: ExtendedCommitmentStatus }[];
};
type FxType = (arg: [id: string, slots: SlotContent[]]) => CustomRow;

export const getFakeSlot = (leader: string, newSlotId: number): ExtendedSlotContent => ({
  leader,
  slot: newSlotId,
  totalTransactionsFiltered: 0,
  feeLevels: [0, 0, 0],
  feeAverage: 0,
  commitment: "scheduled",
  totalUnitsConsumed: 0,
  totalFee: 0,
  hash: "",
  totalTransactions: 0,
  totalTransactionsVote: 0,
  height: 0,
  time: 0,
});

function fillGaps(list: SlotContent[]): ExtendedSlotContent[] {
  if (!list.length) return list;
  if (list.length === 4) return list;
  const groupIdx = ((list?.[0]?.slot || 0) / 4) | 0;

  const isFirst = list.find((elt) => (elt.slot / 4) % 1 === 0);
  const isSecond = list.find((elt) => (elt.slot / 4) % 1 === 0.25);
  const isThird = list.find((elt) => (elt.slot / 4) % 1 === 0.5);
  const isFourth = list.find((elt) => (elt.slot / 4) % 1 === 0.75);

  const newLeader = list?.[0]?.leader || "UNKNOWN";
  const newList: ExtendedSlotContent[] = [];
  isFirst ? newList.push({ ...isFirst }) : newList.push(getFakeSlot(newLeader, groupIdx * 4));
  isSecond ? newList.push({ ...isSecond }) : newList.push(getFakeSlot(newLeader, groupIdx * 4 + 1));
  isThird ? newList.push({ ...isThird }) : newList.push(getFakeSlot(newLeader, groupIdx * 4 + 2));
  isFourth ? newList.push({ ...isFourth }) : newList.push(getFakeSlot(newLeader, groupIdx * 4 + 3));
  const newListSorted = newList.sort((a, b) => b.slot - a.slot);

  if (newListSorted.filter(Boolean).length < 4) debugger;

  return newListSorted;
}

export const prepareValidatorRow: FxType = ([id, rawSlots]) => {
  const slots = fillGaps(rawSlots);
  const leader = slots[0]?.leader || "UNKNOWN";
  return prepareSingeRow(id, leader, slots);
};
export function prepareSingeRow(
  id: string,
  leader: string,
  slots: ExtendedSlotContent[]
): CustomRow {
  return {
    id,
    leader,
    slots: slots.map((elt) => ({
      commitment: elt.commitment,
      slot: elt.slot,
    })),
    transactions: slots.map((elt) => {
      const total = elt.totalTransactions;
      const votedDiff = elt.totalTransactions - elt.totalTransactionsVote;
      const filtered = elt.totalTransactionsFiltered;
      return {
        filtered,
        votedDiff,
        total,
      };
    }),
    computeUnits: slots.map((elt) => {
      const amount = elt.totalUnitsConsumed;
      const percent = elt.totalUnitsConsumed / 48_000_000;
      return {
        amount,
        percent,
      };
    }),
    earnedSol: slots.map((elt) => elt.totalFee / 1e9),
    averageFee: slots.map((elt) => elt.feeAverage),
    fee0: slots.map((elt) => elt.feeLevels[0] || 0),
    fee1: slots.map((elt) => elt.feeLevels[1] || 0),
    fee2: slots.map((elt) => elt.feeLevels[2] || 0),
  };
}
