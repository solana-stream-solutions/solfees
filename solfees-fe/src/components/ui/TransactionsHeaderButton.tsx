import { CSSProperties, useCallback, useMemo, useRef } from "react";
import { IconFunnel } from "@consta/icons/IconFunnel";
import { Button } from "@consta/uikit/Button";
import { useWebSocketStore } from "../../store/websocketStore.ts";
import { ContextMenu, ContextMenuItemDefault } from "@consta/uikit/ContextMenu";
import { useFlag } from "@consta/uikit/useFlag";
import { IconEdit } from "@consta/icons/IconEdit";
import { IconRemoveFromComparison } from "@consta/icons/IconRemoveFromComparison";
import { useShallow } from "zustand/react/shallow";

interface CustomItem extends ContextMenuItemDefault {
  value: "reset" | "update";
}

const items: CustomItem[] = [
  {
    value: "reset",
    label: "Reset filters",
    leftIcon: IconRemoveFromComparison,
    status: "alert",
  },
  {
    value: "update",
    label: "Change filters",
    leftIcon: IconEdit,
  },
];

const redStyle = {
  "--button-color": "red",
  "--button-color-hover": "darkred",
} as unknown as CSSProperties;

interface Props {
  onEditKeys: () => void;
}

export const TransactionsHeaderButton = ({ onEditKeys }: Props) => {
  const readonlyKeys = useWebSocketStore(useShallow((state) => state.readonlyKeys.length));
  const readwriteKeys = useWebSocketStore(useShallow((state) => state.readwriteKeys.length));
  const updateRo = useWebSocketStore(useShallow((state) => state.updateReadonlyKeys));
  const updateRw = useWebSocketStore(useShallow((state) => state.updateReadwriteKeys));
  const updateSubscription = useWebSocketStore(useShallow((state) => state.updateSubscription));
  const [isOpen, isOpenControls] = useFlag(false);
  const ref = useRef(null);

  const isTransactionsApplied = useMemo(() => {
    return !!readwriteKeys || !!readonlyKeys;
  }, [readwriteKeys, readonlyKeys]);
  const onItemClick = useCallback(
    (item: CustomItem) => {
      if (item.value === "update") onEditKeys();
      if (item.value === "reset") {
        updateRo([]);
        updateRw([]);
        updateSubscription();
      }
      isOpenControls.off();
    },
    [onEditKeys, updateRo, updateRw, updateSubscription, isOpenControls]
  );

  if (isTransactionsApplied)
    return (
      <>
        <Button
          ref={ref}
          style={redStyle}
          view="clear"
          size="s"
          onlyIcon
          iconRight={IconFunnel}
          onClick={isOpenControls.toggle}
        />
        <ContextMenu
          className="CustomContextMenu"
          onClickOutside={isOpenControls.off}
          isOpen={isOpen}
          items={items}
          anchorRef={ref}
          direction="downCenter"
          onItemClick={onItemClick}
        />
      </>
    );

  return <Button view="clear" size="s" onlyIcon iconRight={IconFunnel} onClick={onEditKeys} />;
};
