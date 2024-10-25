import { CommitmentStatus, SlotContent } from "../store/websocketStore.ts";

export type CustomRow = {
  id: string;
  leader: string;
  slots: {
    commitment: CommitmentStatus;
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
type FxType = (arg: [id: string, slots: SlotContent[]]) => CustomRow;

const getFakeSlot = (leader: string, newSlotId: number): SlotContent => ({
  leader,
  slot: newSlotId,
  totalTransactionsFiltered: 0,
  feeLevels: [0, 0, 0],
  feeAverage: 0,
  commitment: "fake" as "confirmed",
  totalUnitsConsumed: 0,
  totalFee: 0,
  hash: "",
  totalTransactions: 0,
  totalTransactionsVote: 0,
  height: 0,
  time: 0,
});

function fillGaps(list: SlotContent[]): SlotContent[] {
  if (!list.length) return list;
  if (list.length === 4) return list;
  const groupIdx = ((list?.[0]?.slot || 0) / 4) | 0;

  const isFirst = list.find((elt) => (elt.slot / 4) % 1 === 0);
  const isSecond = list.find((elt) => (elt.slot / 4) % 1 === 0.25);
  const isThird = list.find((elt) => (elt.slot / 4) % 1 === 0.5);
  const isFourth = list.find((elt) => (elt.slot / 4) % 1 === 0.75);

  const newLeader = list?.[0]?.leader || "UNKNOWN";
  const newList: typeof list = [];
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
};
