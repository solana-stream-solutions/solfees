import { IconPropView } from "@consta/icons/Icon";
import { IconCheck } from "@consta/icons/IconCheck";
import { IconAllDone } from "@consta/icons/IconAllDone";
import { AnimateIconBase } from "@consta/icons/AnimateIconBase";
import { IconProcessing } from "@consta/icons/IconProcessing";
import { IconWatchStroked } from "@consta/icons/IconWatchStroked";
import { CustomRow, ExtendedCommitmentStatus } from "../../common/prepareValidatorRow.ts";
import { IconLoading } from "@consta/icons/IconLoading";
import { SlotWithCopy } from "./SlotWithCopy.tsx";

interface Props {
  items: CustomRow["slots"];
}

type ComProps = {
  value: CustomRow["slots"][number]["commitment"];
};

const statuses: ExtendedCommitmentStatus[] = [
  "processed",
  "confirmed",
  "finalized",
  "scheduled",
  "next-leader",
];
const colors: IconPropView[] = ["link", "primary", "success", "ghost", "disabled"];
const icons = [IconProcessing, IconCheck, IconAllDone, IconLoading, IconWatchStroked];
export const AnimateIconBaseIcons = ({ value }: ComProps) => {
  const idx = statuses.findIndex((elt) => elt === value);
  return (
    <AnimateIconBase
      className="shrink-0"
      view={colors[idx] || "alert"}
      icons={icons}
      activeIndex={icons[idx] ? idx : 0}
    />
  );
};

export const Slots = ({ items }: Props) => {
  return (
    <div className="px-3">
      {items.map((elt) => (
        <span
          key={elt.slot}
          className="flex flex-nowrap justify-between gap-1 items-center relative"
        >
          <AnimateIconBaseIcons value={elt.commitment} />
          <SlotWithCopy value={elt.slot} />
        </span>
      ))}
    </div>
  );
};
