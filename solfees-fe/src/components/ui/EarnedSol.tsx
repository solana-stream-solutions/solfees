import { Text } from "@consta/uikit/Text";
import { CustomRow } from "../../common/prepareValidatorRow.ts";

interface Props {
  list: CustomRow["earnedSol"];
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
export const EarnedSol = ({ list }: Props) => {
  return (
    <div className="px-3 text-right">
      {list.map((elt, idx) => (
        <Text key={idx} font="mono">
          {formatValue(elt)}
        </Text>
      ))}
    </div>
  );
};
