import { Text } from "@consta/uikit/Text";

interface Props {
  list: number[];
}
export const SimpleCell = ({ list }: Props) => {
  return (
    <div className="px-3 text-right">
      {list.map((elt, idx) => (
        <Text key={idx} font="mono">
          {elt.toLocaleString("en-US", { maximumFractionDigits: 2 })}
        </Text>
      ))}
    </div>
  );
};
