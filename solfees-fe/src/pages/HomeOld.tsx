// @ts-nocheck
import type { FunctionComponent } from "../common/types";
import {
  CSSProperties,
  ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
} from "react";
import { SlotContent, useWebSocketStore } from "../store/websocketStore.ts";
import { Table, TableColumn, TableFilters } from "@consta/uikit/Table";
import { IconInfoCircle } from "@consta/icons/IconInfoCircle";
import { IconFunnel } from "@consta/icons/IconFunnel";
import { IconEdit } from "@consta/icons/IconEdit";
import { IconAdd } from "@consta/icons/IconAdd";
import { withTooltip } from "@consta/uikit/withTooltip";
import { Button } from "@consta/uikit/Button";
import { TooltipProps } from "@consta/uikit/__internal__/src/hocs/withTooltip/withTooltip";
import { Text } from "@consta/uikit/Text";
import { Modal } from "@consta/uikit/Modal";
import { useFlag } from "@consta/uikit/useFlag";
import { TextField } from "@consta/uikit/TextField";
import { Chips } from "@consta/uikit/Chips";
import { Validator } from "../components/ui/Validator.tsx";
import { Slots } from "../components/ui/Slots.tsx";
import { ComputeUnits } from "../components/ui/ComputeUnits.tsx";
import { SimpleCell } from "../components/ui/SimpleCell.tsx";
import { useNavigate } from "@tanstack/react-router";
import { Epoch } from "../components/ui/Epoch.tsx";
import { IconFeed } from "../components/ui/IconFeed.tsx";
import { ModalFilter } from "../components/ui/ModalFilter.tsx";
import { percentFromStore } from "../common/utils.ts";
import { ModalFee } from "../components/ui/ModalFee.tsx";
import { CustomRow } from "../common/prepareValidatorRow.ts";

const ButtonWithTooltip = withTooltip({ content: "Top tooltip" })(Button);

const InfoButton = (props: TooltipProps): ReactNode => {
  return (
    <ButtonWithTooltip
      as="span"
      iconSize="s"
      onlyIcon={true}
      view="clear"
      iconRight={IconInfoCircle}
      tooltipProps={props}
    />
  );
};

function buildTransactions(slots: SlotContent[]) {
  const first = slots.map((elt) => elt.totalTransactions - elt.totalTransactionsVote);
  const maxFirst = `${Math.max(...first)}`.length;
  const second = slots.map((elt) => elt.totalTransactions);
  const maxSecond = `${Math.max(...second)}`.length;
  const aligned = slots.map((_, idx) => {
    const colOne = `${first[idx]}`.padStart(maxFirst, " ");
    const colSecond = `${second[idx]}`.padStart(maxSecond, " ");
    return colOne + " / " + colSecond;
  });
  return aligned;
}

type TableProps = {
  onEditFee: (idx: number) => void;
  onEditKeys: () => void;
};
const CustomTable = ({ onEditFee, onEditKeys }: TableProps) => {
  const slots2 = useWebSocketStore((state) => state.slots2);
  const percents = useWebSocketStore((state) => state.percents);
  const readonlyKeys = useWebSocketStore((state) => state.readonlyKeys);
  const readwriteKeys = useWebSocketStore((state) => state.readwriteKeys);

  const memoFee0 = useCallback(() => onEditFee(0), [onEditFee]);
  const memoFee1 = useCallback(() => onEditFee(1), [onEditFee]);

  const rowsFromSocket2 = useMemo(() => {
    const unsorted = slots2 as Record<string, SlotContent[]>;
    return Object.entries(unsorted)
      .sort((a, b) => Number(a[0]) - Number(b[0]))
      .map(([id, slots]) => {
        const leader = slots[0]?.leader || "UNKNOWN";
        return {
          id,
          leader,
          slots: slots.map((elt) => ({
            commitment: elt.commitment,
            slot: elt.slot,
          })),
          transactions: buildTransactions(slots),
          computeUnits: slots.map((elt) => ({
            amount: elt.totalUnitsConsumed.toLocaleString("en-US"),
            percent: ((elt.totalUnitsConsumed / 48_000_000) * 100).toFixed(2),
          })),
          earnedSol: slots.map((elt) => elt.totalFee),
          averageFee: slots.map((elt) => elt.feeAverage.toFixed(2)),
          fee0: slots.map((elt) => elt.feeLevels[0] || 0),
          fee1: slots.map((elt) => elt.feeLevels[1] || 0),
          add: [],
        };
      });
  }, [slots2]);

  const isTransactionsApplied = useMemo(() => {
    return !!readwriteKeys.length || !!readonlyKeys.length;
  }, [readwriteKeys.length, readonlyKeys.length]);
  const columns: TableColumn<(typeof rowsFromSocket2)[number]>[] = [
    {
      width: "auto" as unknown as number,
      title: "Validator",
      accessor: "leader",
      renderCell: (row) => <Validator leader={row.leader} />,
    },
    {
      width: "auto" as unknown as number,
      align: "right",
      title: "Slots",
      accessor: "slots",
      renderCell: (row) => <Slots items={row.slots} />,
    },
    {
      title: "Transactions",
      accessor: "transactions",
      align: "right",
      control: () => (
        <Button
          style={
            {
              "--button-color": isTransactionsApplied ? "red" : undefined,
              "--button-color-hover": isTransactionsApplied ? "darkred" : undefined,
            } as unknown as CSSProperties
          }
          view="clear"
          size="s"
          onlyIcon
          iconRight={IconFunnel}
          onClick={onEditKeys}
        />
      ),
      renderCell: (row) => (
        <span>
          {row.transactions.map((elt) => (
            <Text font="mono" className="whitespace-pre">
              {elt}
            </Text>
          ))}
        </span>
      ),
    },
    {
      title: "Compute Units",
      width: 190,
      align: "right",
      accessor: "computeUnits",
      control: ({ column }) => (
        <InfoButton content={column.title as string} direction={"downCenter"} />
      ),
      renderCell: (row) => (
        <ComputeUnits items={row.computeUnits as unknown as CustomRow["computeUnits"]} />
      ),
    },
    {
      title: "Earned SOL",
      align: "right",
      accessor: "earnedSol",
      control: ({ column }) => (
        <InfoButton content={column.title as string} direction={"downCenter"} />
      ),
      renderCell: (row) => <SimpleCell list={row.earnedSol} />,
    },
    {
      title: "Average Fee",
      accessor: "averageFee",
      align: "right",
      control: ({ column }) => (
        <InfoButton content={column.title as string} direction={"downCenter"} />
      ),
      renderCell: (row) => <SimpleCell list={row.averageFee as unknown as number[]} />,
    },
    {
      title: "Fee p" + percentFromStore(percents[0]),
      accessor: "fee0",
      align: "right",
      control: () => (
        <Button
          as="span"
          iconSize="s"
          onlyIcon={true}
          view="clear"
          iconRight={IconEdit}
          onClick={memoFee0}
        />
      ),
      renderCell: (row) => <SimpleCell list={row.fee0} />,
    },
    {
      title: "Fee p" + percentFromStore(percents[1]),
      accessor: "fee1",
      align: "right",
      control: () => (
        <Button
          as="span"
          iconSize="s"
          onlyIcon={true}
          view="clear"
          iconRight={IconEdit}
          onClick={memoFee1}
        />
      ),
      renderCell: (row) => <SimpleCell list={row.fee1} />,
    },
    {
      title: "Add",
      accessor: "add",
      control: () => (
        <Button as="span" iconSize="s" onlyIcon={true} view="clear" iconRight={IconAdd} />
      ),
    },
  ];

  if (!rowsFromSocket2.length)
    return <span className="w-full text-center text-xl font-bold">Loading...</span>;

  return <Table className="w-full2" columns={columns} rows={[...rowsFromSocket2].reverse()} />;
};
export const HomeOld = (): FunctionComponent => {
  const [filterModalShown, filterModalControls] = useFlag(false);
  const [editedFeeIdx, setEditedFeeIdx] = useState(-1);
  const navigate = useNavigate();

  return (
    <>
      <button
        className="absolute top-2 left-2 rounded bg-amber-300"
        onClick={() => navigate({ to: "/homeNew" })}
      >
        click to view Table from @consta\TABLE (new)
      </button>
      <div className="px-20 py-5 bg-white w-full flex-col justify-start items-start gap-8 inline-flex">
        <div className="self-stretch justify-between items-center inline-flex">
          <div className="justify-start items-center gap-2 flex">
            <IconFeed className="w-5 h-5 relative"></IconFeed>
            <div className="text-center text-[#002033] text-2xl font-semibold font-['Inter'] leading-[31.20px]">
              Solfees
            </div>
            <div className="text-center text-[#002033]/60 text-sm font-normal font-['Inter'] leading-[21px]">
              Solana Fees Tracker
            </div>
          </div>
          <Epoch />
        </div>
        <CustomTable onEditFee={setEditedFeeIdx} onEditKeys={filterModalControls.on} />
        <ModalFee
          editedFeeIdx={editedFeeIdx}
          isVisible={editedFeeIdx >= 0}
          onClose={() => setEditedFeeIdx(-1)}
        />
        <ModalFilter isVisible={filterModalShown} onClose={filterModalControls.off} />
      </div>
    </>
  );
};
