import { withTooltip } from "@consta/uikit/withTooltip";
import { Button } from "@consta/uikit/Button";
import { TooltipProps } from "@consta/uikit/__internal__/src/hocs/withTooltip/withTooltip";
import { ReactNode, useCallback, useDeferredValue, useMemo } from "react";
import { IconInfoCircle } from "@consta/icons/IconInfoCircle";
import { SlotContent, useWebSocketStore } from "../../store/websocketStore.ts";
import { useShallow } from "zustand/react/shallow";
import { useScheduleStore } from "../../store/scheduleStore.ts";
import {
  getFakeSlot,
  prepareSingeRow,
  prepareValidatorRow,
} from "../../common/prepareValidatorRow.ts";
import { Table, TableColumn } from "@consta/table/Table";
import { Validator } from "./Validator.tsx";
import { Slots } from "./Slots.tsx";
import { HeaderDataCell } from "@consta/table/HeaderDataCell";
import { TransactionsHeaderButton } from "./TransactionsHeaderButton.tsx";
import { Transactions } from "./Transactions.tsx";
import { ComputeUnits } from "./ComputeUnits.tsx";
import { EarnedSol } from "./EarnedSol.tsx";
import { SimpleCell } from "./SimpleCell.tsx";
import { percentFromStore } from "../../common/utils.ts";
import { IconEdit } from "@consta/icons/IconEdit";

const ButtonWithTooltip = withTooltip({ content: "Top Tooltip" })(Button);

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

type TableProps = {
  onEditFee: (idx: number) => void;
  onEditKeys: () => void;
};

export const CustomTable = ({ onEditFee, onEditKeys }: TableProps) => {
  const slots2 = useWebSocketStore(useShallow((state) => state.slots2));
  const percents = useWebSocketStore(useShallow((state) => state.percents));
  const indices = useScheduleStore(useShallow((state) => state.indices));
  const leaders = useScheduleStore(useShallow((state) => state.leaders));

  const memoFee0 = useCallback(() => onEditFee(0), [onEditFee]);
  const memoFee1 = useCallback(() => onEditFee(1), [onEditFee]);
  const memoFee2 = useCallback(() => onEditFee(2), [onEditFee]);

  const rowsFromSocket2 = useMemo(() => {
    const unsorted = slots2 as Record<string, SlotContent[]>;
    const result = Object.entries(unsorted)
      .sort((a, b) => Number(a[0]) - Number(b[0]))
      .map(prepareValidatorRow);
    // we always add next leader, but pick only one slot for it
    const idx = Math.max(...Object.keys(slots2).map(Number));
    const lastSlot = slots2[idx]?.[0]?.slot || 0;
    if (lastSlot) {
      const nextSlotNumber = (((lastSlot / 4) | 0) + 1) * 4;
      const nextLeaderIndex = indices[nextSlotNumber % 432_000] || 0;
      const nextLeader = leaders[nextLeaderIndex] || "";

      const nextSlotContent = getFakeSlot(nextLeader, nextSlotNumber);
      nextSlotContent.commitment = "next-leader";
      const nextRow = prepareSingeRow(`scheduled-${nextSlotNumber}`, nextLeader, [nextSlotContent]);
      result.push(nextRow);
    }
    return [...result].reverse();
  }, [slots2, indices, leaders]);

  const columns: TableColumn<(typeof rowsFromSocket2)[number]>[] = useMemo(() => {
    return [
      {
        minWidth: 150,
        title: "Validator",
        accessor: "leader",
        renderCell: ({ row }) => <Validator slots={row.slots} leader={row.leader} />,
        colSpan: ({ row }) =>
          row.slots.some((elt) => elt.commitment === "next-leader") ? "end" : undefined,
      },
      {
        minWidth: 170,
        title: "Slots",
        accessor: "slots",
        renderCell: ({ row }) => <Slots items={row.slots} />,
      },
      {
        minWidth: 160,
        title: "Transactions",
        accessor: "transactions",
        renderHeaderCell: ({ title }) => (
          <HeaderDataCell controlRight={[<TransactionsHeaderButton onEditKeys={onEditKeys} />]}>
            {title}
          </HeaderDataCell>
        ),
        renderCell: ({ row }) => <Transactions slots={row.slots} items={row.transactions} />,
      },
      {
        title: "Compute Units",
        minWidth: 216,
        accessor: "computeUnits",
        renderHeaderCell: ({ title }) => (
          <HeaderDataCell
            controlRight={[<InfoButton content={title as string} direction={"downCenter"} />]}
          >
            {title}
          </HeaderDataCell>
        ),
        renderCell: ({ row }) => <ComputeUnits slots={row.slots} items={row.computeUnits} />,
      },
      {
        minWidth: 160,
        title: "Earned SOL",
        accessor: "earnedSol",
        renderHeaderCell: ({ title }) => (
          <HeaderDataCell
            controlRight={[<InfoButton content={title as string} direction={"downCenter"} />]}
          >
            {title}
          </HeaderDataCell>
        ),
        renderCell: ({ row }) => <EarnedSol slots={row.slots} items={row.earnedSol} />,
      },
      {
        minWidth: 160,
        accessor: "averageFee",
        title: "Average Fee",

        renderHeaderCell: ({ title }) => (
          <HeaderDataCell
            controlRight={[<InfoButton content={title as string} direction={"downCenter"} />]}
          >
            {title}
          </HeaderDataCell>
        ),
        renderCell: ({ row }) => <SimpleCell slots={row.slots} items={row.averageFee} />,
      },
      {
        minWidth: 160,
        title: "Fee p" + percentFromStore(percents[0]),
        accessor: "fee0",
        renderHeaderCell: ({ title }) => (
          <HeaderDataCell
            controlRight={[
              <Button
                as="span"
                iconSize="s"
                onlyIcon={true}
                view="clear"
                iconRight={IconEdit}
                onClick={memoFee0}
              />,
            ]}
          >
            {title}
          </HeaderDataCell>
        ),
        renderCell: ({ row }) => <SimpleCell slots={row.slots} items={row.fee0} />,
      },
      {
        minWidth: 160,
        title: "Fee p" + percentFromStore(percents[1]),
        accessor: "fee1",
        renderHeaderCell: ({ title }) => (
          <HeaderDataCell
            controlRight={[
              <Button
                as="span"
                iconSize="s"
                onlyIcon={true}
                view="clear"
                iconRight={IconEdit}
                onClick={memoFee1}
              />,
            ]}
          >
            {title}
          </HeaderDataCell>
        ),
        renderCell: ({ row }) => <SimpleCell slots={row.slots} items={row.fee1} />,
      },
      {
        minWidth: 160,
        title: "Fee p" + percentFromStore(percents[2]),
        accessor: "fee2",
        renderHeaderCell: ({ title }) => (
          <HeaderDataCell
            controlRight={[
              <Button
                as="span"
                iconSize="s"
                onlyIcon={true}
                view="clear"
                iconRight={IconEdit}
                onClick={memoFee2}
              />,
            ]}
          >
            {title}
          </HeaderDataCell>
        ),
        renderCell: ({ row }) => <SimpleCell slots={row.slots} items={row.fee2} />,
      },
    ];
  }, [onEditKeys, percents, memoFee0, memoFee1, memoFee2]);

  const deferredValue = useDeferredValue(rowsFromSocket2);

  // For testing with simple table layout (no components, only HTML tags)
  const isSimple = false;
  if (isSimple)
    return (
      <>
        <div className="w-full overflow-x-auto">
          {rowsFromSocket2.map((row, _idx) => {
            const key = `${row.leader}-${((row.slots[0]?.slot || 0) / 4) | 0}`;
            return (
              <div className="border flex flex-row gap-1" key={key}>
                <div className="bg-red-100">{row.leader}</div>
                <div>
                  {row.slots.map((slot) => (
                    <div className="bg-green-100" key={slot.slot}>
                      {slot.commitment}-{slot.slot}
                    </div>
                  ))}
                </div>
                <div>
                  {row.fee0.map((elt, idx) => (
                    <div className="bg-blue-100" key={idx}>
                      {elt}
                    </div>
                  ))}
                </div>
                <div>
                  {row.fee1.map((elt, idx) => (
                    <div className="bg-red-100" key={idx}>
                      {elt}
                    </div>
                  ))}
                </div>
                <div>
                  {row.averageFee.map((elt, idx) => (
                    <div className="bg-green-100" key={idx}>
                      {elt}
                    </div>
                  ))}
                </div>
                <div>
                  {row.earnedSol.map((elt, idx) => (
                    <div className="bg-blue-100" key={idx}>
                      {elt}
                    </div>
                  ))}
                </div>
                <div>
                  {row.computeUnits.map((elt, idx) => (
                    <div className="bg-red-100" key={idx}>
                      {elt.amount}-{elt.percent}
                    </div>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      </>
    );

  if (!rowsFromSocket2.length)
    return <span className="w-full h-full text-center text-xl font-bold">Loading...</span>;

  return (
    <div className="w-full h-full overflow-x-auto">
      <Table
        className="overflow-scroll"
        columns={columns}
        rows={deferredValue}
        style={{ maxHeight: "100%" }}
        virtualScroll={false}
        resizable={undefined}
      />
    </div>
  );
};
