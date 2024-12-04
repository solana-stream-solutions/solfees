import { Text } from "@consta/uikit/Text";
import { CustomRow } from "../../common/prepareValidatorRow.ts";
import { isReal } from "../../common/isReal.ts";

interface Props {
  items: number[];
  slots: CustomRow["slots"];
}
export const SimpleCell = ({ items, slots }: Props) => {
  return (
    <div className="px-3 text-right">
      {items.map((rawElt, idx) => {
        // this is crutch, until BE fixes null for "feeAverage"
        const elt = rawElt ?? 0;
        const currentSlot = slots[idx];
        const isFilled = currentSlot ? isReal(currentSlot) : false;

        return (
          <Text key={idx} font="mono" className="whitespace-pre">
            {isFilled ? elt.toLocaleString("en-US", { maximumFractionDigits: 2 }) : " "}
          </Text>
        );
      })}
    </div>
  );
};
