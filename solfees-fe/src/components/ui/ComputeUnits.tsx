import {Text} from "@consta/uikit/Text";
import {CustomRow} from "../../common/prepareValidatorRow.ts";
import {nFormatter} from "../../common/nFormatter.ts";

interface Props {
  items: CustomRow['computeUnits']
}

// computeUnits: slots.map(elt => ({
//   amount: elt.totalUnitsConsumed.toLocaleString('en-US', {
//     minimumFractionDigits: 2,
//     maximumFractionDigits: 2,
//   }),
//   percent: (elt.totalUnitsConsumed / 48_000_000 * 100).toFixed(2),
// })),

const percentFormatter = new Intl.NumberFormat('unknown', {style: 'percent',maximumFractionDigits: 2, minimumFractionDigits: 2, minimumIntegerDigits: 2})



export const ComputeUnits = ({items}: Props) => {
  return <div className="px-3 min-w-0">
    {items.map((elt, idx) => <div key={elt.amount === 0 ? idx : elt.amount}
      className="flex justify-end text-right gap-1 shrink-0">
      <Text font="mono">{nFormatter(elt.amount, 2, true)}</Text><Text font="mono">({percentFormatter.format(elt.percent).replace(/\s/g, '')})</Text></div>)}
  </div>
}
