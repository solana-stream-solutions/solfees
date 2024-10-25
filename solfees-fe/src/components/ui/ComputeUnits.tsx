import { Text } from "@consta/uikit/Text";
import { CustomRow } from "../../common/prepareValidatorRow.ts";
import { withTooltip } from "@consta/uikit/withTooltip";
import { isReal } from "../../common/isReal.ts";

interface Props {
  items: CustomRow["computeUnits"];
  slots: CustomRow["slots"];
}

const TextWithTooltip = withTooltip({ content: "Top tooltip" })(Text);

const percentFormatter = new Intl.NumberFormat("en-US", {
  style: "percent",
  maximumFractionDigits: 2,
  minimumFractionDigits: 2,
  minimumIntegerDigits: 2,
});

const tooltipFormatter = new Intl.NumberFormat("en-US", {
  style: "decimal",
  maximumFractionDigits: 2,
});

const amountFormatter = (amount: number): string => {
  const divided = amount / 1e6;
  const result = divided.toFixed(3) + "M";
  return result;
};

export const ComputeUnits = ({ items, slots }: Props) => {
  return (
    <div className="px-3 min-w-0">
      {items.map((elt, idx) => {
        const currentSlot = slots[idx];
        const isFilled = currentSlot ? isReal(currentSlot) : false;

        return (
          <div
            key={elt.amount === 0 ? idx : elt.amount}
            className="flex justify-end whitespace-pre"
          >
            {isFilled ? (
              <TextWithTooltip
                className="flex justify-end text-right gap-1 shrink-0"
                tooltipProps={{
                  content: tooltipFormatter.format(elt.amount),
                  direction: "leftCenter",
                  appearTimeout: 0,
                  exitTimeout: 0,
                }}
              >
                <Text font="mono">{amountFormatter(elt.amount)}</Text>
                <Text font="mono">({percentFormatter.format(elt.percent).replace(/,/g, " ")})</Text>
              </TextWithTooltip>
            ) : (
              <Text font="mono" className="flex-shrink-0">
                {" "}
              </Text>
            )}
          </div>
        );
      })}
    </div>
  );
};
