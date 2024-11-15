import { Text } from "@consta/uikit/Text";
import { CustomRow } from "../../common/prepareValidatorRow.ts";
import { isReal } from "../../common/isReal.ts";

interface Props {
  items: CustomRow["earnedSol"];
  slots: CustomRow["slots"];
}

function formatValue(value: number): string {
  return value > 1
    ? value
        .toLocaleString("unknown", {
          maximumFractionDigits: 9,
          minimumFractionDigits: 9,
        })
        .replace(/\s/g, "")
    : value.toFixed(9);
}
export const EarnedSol = ({ items, slots }: Props) => {
  return (
    <div className="px-3 text-right">
      {items.map((elt, idx) => {
        const currentSlot = slots[idx];
        const isFilled = currentSlot ? isReal(currentSlot) : false;

        return (
          <Text key={idx} font="mono" className="whitespace-pre">
            {isFilled ? formatValue(elt) : " "}
          </Text>
        );
      })}
    </div>
  );
};
