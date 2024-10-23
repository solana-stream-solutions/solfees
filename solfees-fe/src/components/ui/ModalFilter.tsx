import { useCallback, useLayoutEffect, useState } from "react";
import { useWebSocketStore } from "../../store/websocketStore.ts";
import { Modal } from "@consta/uikit/Modal";
import { Text } from "@consta/uikit/Text";
import { TextField } from "@consta/uikit/TextField";
import { Button } from "@consta/uikit/Button";
import { useShallow } from "zustand/react/shallow";

type ModalFilterProps = {
  isVisible: boolean;
  onClose: () => void;
};

export const ModalFilter = ({ isVisible, onClose }: ModalFilterProps) => {
  const [rwValue, setRwValue] = useState("");
  const [roValue, setRoValue] = useState("");
  const savedReadonlyKeys = useWebSocketStore(useShallow((state) => state.readonlyKeys));
  const savedReadwriteKeys = useWebSocketStore(useShallow((state) => state.readwriteKeys));
  const updateRo = useWebSocketStore(useShallow((state) => state.updateReadonlyKeys));
  const updateRw = useWebSocketStore(useShallow((state) => state.updateReadwriteKeys));
  const updateSubscription = useWebSocketStore(useShallow((state) => state.updateSubscription));

  const onClickButton = useCallback(() => {
    // Может быть проблема, что пишет массив с одним элементом "" (пустая строка). Уточнить потом.
    updateRo(roValue.split("\n"));
    updateRw(rwValue.split("\n"));
    updateSubscription();
    onClose();
  }, [updateRo, roValue, updateRw, rwValue, onClose]);
  useLayoutEffect(() => {
    if (isVisible) {
      setRoValue(savedReadonlyKeys.join("\n"));
      setRwValue(savedReadwriteKeys.join("\n"));
    }
  }, [isVisible]);

  const appendRo = useCallback(
    (token: string) =>
      setRoValue((prev) => {
        const isEmpty = !prev.length;
        return isEmpty ? token : prev + "\n" + token;
      }),
    []
  );

  return (
    <Modal
      isOpen={isVisible}
      hasOverlay
      onClickOutside={onClose}
      onEsc={onClose}
      className="flex flex-col gap-4 p-6 min-w-[50%]"
    >
      <Text as="h1" size="2xl" view="primary">
        Filter transactions
      </Text>
      <div>
        <Text as="h2" weight="medium" view="secondary">
          Predefined values:
        </Text>
        <Text
          onClick={() => appendRo("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")}
          as="div"
          view="link"
          className="cursor-pointer hover:underline"
        >
          Tokenkeg
        </Text>
      </div>

      <div className="flex flex-col gap-2">
        <TextField
          className="min-w-80 custom-nowrap-textarea"
          label="Read-write pubkeys"
          labelPosition="top"
          type="textarea"
          onChange={(value) => setRwValue(value || "")}
          value={rwValue}
          placeholder="Pubkeys"
          minRows={3 as unknown as undefined}
          maxRows={10 as unknown as undefined}
        />
      </div>
      <div className="flex flex-col gap-2">
        <TextField
          className="min-w-80 custom-nowrap-textarea"
          label="Read-only pubkeys"
          labelPosition="top"
          type="textarea"
          onChange={(value) => setRoValue(value || "")}
          value={roValue}
          placeholder="Pubkeys"
          minRows={3 as unknown as undefined}
          maxRows={10 as unknown as undefined}
        />
      </div>
      <Button view="primary" label="Done" width="full" onClick={onClickButton} />
    </Modal>
  );
};
