import {useWebSocketStore} from "../../store/websocketStore.ts";
import {useLayoutEffect, useState} from "react";
import {Modal} from "@consta/uikit/Modal";
import {Text} from "@consta/uikit/Text";
import {TextField} from "@consta/uikit/TextField";
import {Chips} from "@consta/uikit/Chips";
import {Button} from "@consta/uikit/Button";
import {percentFromStore} from "../../common/utils.ts";
import {useShallow} from "zustand/react/shallow";

type ModalFeeProps = {
  isVisible: boolean;
  onClose: () => void;
  editedFeeIdx: number;
}

const modalFeeItems = [50, 60, 70, 80, 90].map(elt => ({label: `${elt}`}))
export const ModalFee = ({
                    isVisible,
                    onClose,
                    editedFeeIdx,
                  }: ModalFeeProps) => {
  const fees = useWebSocketStore(useShallow(state => state.percents))
  const updatePercents = useWebSocketStore(useShallow(state => state.updatePercents))
  const updateSubscription = useWebSocketStore(useShallow(state => state.updateSubscription))

  const [feeValue, setFeeValue] = useState('');
  useLayoutEffect(() => {
    if (isVisible) {
      const value = percentFromStore(fees[editedFeeIdx] || 0);
      setFeeValue(`${value}`)
    }
  }, [editedFeeIdx, isVisible])
  const onClickButton = () => {
    const newPercents = fees.map((elt, idx) => {
      if (idx === editedFeeIdx) return +(+feeValue * 100).toFixed(2)|0;
      return elt;
    })
    updatePercents(newPercents)
    updateSubscription();
    onClose();
  }
  return <Modal
    isOpen={isVisible}
    hasOverlay
    onClickOutside={onClose}
    onEsc={onClose}
    className="flex flex-col gap-4 p-6"
  >
    <Text as="h1" size="2xl" view="primary">
      Fee percentile
    </Text>
    <div className="flex flex-col gap-2">
      <TextField
        onChange={value => setFeeValue(value || '')}
        value={feeValue}
        type="number"
        placeholder="1-100"
        incrementButtons={false}
      />
      <Chips interactive items={modalFeeItems} onItemClick={item => setFeeValue(item.label)} size="xs"/>
    </div>
    <Button
      view="primary"
      label="Done"
      width="full"
      onClick={onClickButton}
    />
  </Modal>
}


