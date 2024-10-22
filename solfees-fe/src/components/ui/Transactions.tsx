import {Text} from "@consta/uikit/Text";
import {useWebSocketStore} from "../../store/websocketStore.ts";
import {useShallow} from "zustand/react/shallow";
import {CustomRow} from "../../common/prepareValidatorRow.ts";

interface Props {
  items: CustomRow['transactions']
}

function buildTransactions(slots: CustomRow['transactions'], withFiltered = false) {
  const zero = slots.map(elt => elt.filtered)
  const maxZero = `${Math.max(...zero)}`.length
  const first = slots.map(elt => elt.votedDiff)
  const maxFirst = `${Math.max(...first)}`.length
  const second = slots.map(elt => elt.total)
  const maxSecond = `${Math.max(...second)}`.length
  const aligned = slots.map((_, idx) => {
    const colOne = `${first[idx]}`.padStart(maxFirst, ' ');
    const colSecond = `${second[idx]}`.padStart(maxSecond, ' ');
    let colFiltered = ''
    if (withFiltered) {
      colFiltered = `${zero[idx]}`.padStart(maxZero, ' ');
      colFiltered += ' / '
    }
    return colFiltered + colOne + ' / ' + colSecond
  })
  const hasDupes = new Set(aligned).size !== aligned.length
  const alignedWithKeys = aligned.map((value, idx) => {
    const key = hasDupes ? `${idx}-value` : value
    return {key, value}
  })
  return alignedWithKeys
}

export const Transactions = ({items}: Props) => {
  const hasRW = useWebSocketStore(useShallow(state => !!state.readwriteKeys.length))
  const hasRO = useWebSocketStore(useShallow(state => !!state.readonlyKeys.length))


  return <div className="px-3 text-right">
    {buildTransactions(items, hasRO || hasRW).map(elt => <Text key={elt.key}
                                                               font="mono"
                                                               className="whitespace-pre">{elt.value}</Text>)}
  </div>

}