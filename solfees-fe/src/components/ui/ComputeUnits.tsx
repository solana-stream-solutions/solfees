import { Text } from "@consta/uikit/Text";
import { CustomRow } from "../../common/prepareValidatorRow.ts";
import { withTooltip } from "@consta/uikit/withTooltip";

interface Props {
  items: CustomRow["computeUnits"];
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

export const ComputeUnits = ({ items }: Props) => {
  return (
    <div className="px-3 min-w-0">
      {items.map((elt, idx) => (
        <div key={elt.amount === 0 ? idx : elt.amount} className="flex justify-end">
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
        </div>
      ))}
    </div>
  );
};
