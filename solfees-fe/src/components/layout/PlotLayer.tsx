import { useMemo, useState } from "react";
import { CommitmentStatus, useWebSocketStore } from "../../store/websocketStore.ts";
import { SwitchGroup } from "@consta/uikit/SwitchGroup";
import { ContentType } from "recharts/types/component/Tooltip";
import { CurveType } from "recharts/types/shape/Curve";
import {
  Area,
  Bar,
  CartesianGrid,
  ComposedChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { nFormatter } from "../../common/nFormatter.ts";

type Item = {
  key: number;
  value: string;
};

const items: Item[] = [
  {
    key: 0,
    value: "fee0",
  },
  {
    key: 1,
    value: "fee1",
  },
  {
    key: 2,
    value: "fee2",
  },
];

export const PlotLayer = () => {
  const [value, setValue] = useState<Item[] | null>(null);
  const percents = useWebSocketStore((state) => state.percents);
  const getItemLabel = (arg: Item): string =>
    "p" + ((percents[arg.key] || 0) / 100).toLocaleString("en-US", { maximumFractionDigits: 2 });

  return (
    <>
      <div className="flex flex-nowrap gap-4 w-full">
        <div className="w-full">
          <ExampleAreaOneBar />
        </div>
        <div className="w-full">
          <ExampleAreaTwoBar />
        </div>
        <div className="w-full">
          <SwitchGroup
            value={value}
            items={items}
            getItemLabel={getItemLabel}
            onChange={(arg) => setValue(arg as Item[] | null)}
            direction="row"
            name="FeesControls"
          />
          <ExampleAreaThree items={(value || []).map((elt) => elt.value)} />
        </div>
      </div>
    </>
  );
};

function tickFormatter(value: number, _index: number): string {
  return nFormatter(value);
}

function tickFormatter2(value: number, _index: number): string {
  const newValue = value / 1e9;
  return newValue.toFixed(2);
}

const CustomTooltip: ContentType<any, any> = ({ active, payload }) => {
  if (active && payload && payload.length) {
    if (payload[0]) {
      const elt = payload[0].payload;
      return (
        <div className="recharts-default-tooltip">
          <p className="recharts-tooltip-label">
            Slot: {elt.x.toLocaleString("en-US")} ({elt.commitment})
          </p>
          <ul className="recharts-tooltip-item-list">
            <li className="recharts-tooltip-item">
              <span className="recharts-tooltip-item-name">Tracked Items</span>
              <span className="recharts-tooltip-item-separator"> : </span>
              <span className="recharts-tooltip-item-value">{elt.y.toLocaleString("en-US")}</span>
            </li>
            <li className="recharts-tooltip-item">
              <span className="recharts-tooltip-item-name">DEBUG processed</span>
              <span className="recharts-tooltip-item-separator"> : </span>
              <span className="recharts-tooltip-item-value">{elt.value.processed}</span>
            </li>
            <li className="recharts-tooltip-item">
              <span className="recharts-tooltip-item-name">DEBUG confirmed</span>
              <span className="recharts-tooltip-item-separator"> : </span>
              <span className="recharts-tooltip-item-value">{elt.value.confirmed}</span>
            </li>
            <li className="recharts-tooltip-item">
              <span className="recharts-tooltip-item-name">DEBUG finalized</span>
              <span className="recharts-tooltip-item-separator"> : </span>
              <span className="recharts-tooltip-item-value">{elt.value.finalized}</span>
            </li>
          </ul>
        </div>
      );
    }
  }

  return null;
};
const CustomTooltip2: ContentType<any, any> = ({ active, payload }) => {
  if (active && payload && payload.length) {
    if (payload[0]) {
      const elt = payload[0].payload;
      const value = elt.y / 1e9;
      const valueInTooltip =
        value > 1
          ? value
              .toLocaleString("unknown", {
                maximumFractionDigits: 9,
                minimumFractionDigits: 9,
              })
              .replace(/\s/g, "")
          : value.toFixed(9);
      return (
        <div className="recharts-default-tooltip">
          <p className="recharts-tooltip-label">
            Slot: {elt.x.toLocaleString("en-US")} ({elt.commitment})
          </p>
          <ul className="recharts-tooltip-item-list">
            <li className="recharts-tooltip-item">
              <span className="recharts-tooltip-item-name">Earned</span>
              <span className="recharts-tooltip-item-separator"> : </span>
              <span className="recharts-tooltip-item-value">{valueInTooltip}</span>
            </li>
            <li className="recharts-tooltip-item">
              <span className="recharts-tooltip-item-name">DEBUG processed</span>
              <span className="recharts-tooltip-item-separator"> : </span>
              <span className="recharts-tooltip-item-value">{elt.value.processed}</span>
            </li>
            <li className="recharts-tooltip-item">
              <span className="recharts-tooltip-item-name">DEBUG confirmed</span>
              <span className="recharts-tooltip-item-separator"> : </span>
              <span className="recharts-tooltip-item-value">{elt.value.confirmed}</span>
            </li>
            <li className="recharts-tooltip-item">
              <span className="recharts-tooltip-item-name">DEBUG finalized</span>
              <span className="recharts-tooltip-item-separator"> : </span>
              <span className="recharts-tooltip-item-value">{elt.value.finalized}</span>
            </li>
          </ul>
        </div>
      );
    }
  }

  return null;
};

type PlotInfo = {
  x: number;
  commitment: CommitmentStatus;
  y: number;
  value: Record<CommitmentStatus, null | number>;
};
type FeesInfo = {
  x: number;
  commitment: CommitmentStatus;
  y: number;
  fee0: number | null;
  fee1: number | null;
  fee2: number | null;
};
export const ExampleAreaOneBar = () => {
  const slots2 = useWebSocketStore((state) => state.slots2);

  const [_type, _setType] = useState<CurveType>("monotone");

  const data = useMemo(() => {
    const entries = Object.entries(slots2);
    const packedData = entries.reduce<PlotInfo[]>((acc, [_groupIdx, slots]) => {
      const chunk = slots.map((elt) => {
        const obj = {
          x: elt.slot,
          y: elt.totalUnitsConsumed,
          commitment: elt.commitment,
          value: {
            confirmed: null,
            finalized: null,
            processed: null,
          },
        } satisfies PlotInfo;
        obj.value[elt.commitment] = obj.y as unknown as null;
        return obj;
      });
      const sorted = chunk.sort((a, b) => a.x - b.x);
      acc.push(...sorted);
      return acc;
    }, []);
    return packedData;
  }, [slots2]);
  return (
    <div
      style={{
        width: "100%",
      }}
    >
      <ResponsiveContainer width="100%" height={200}>
        <ComposedChart
          data={data}
          margin={{
            top: 0,
            right: 0,
            left: 0,
            bottom: 0,
          }}
        >
          <CartesianGrid strokeDasharray="3 3" vertical={false} />
          <YAxis scale="auto" tickFormatter={tickFormatter} domain={[0, 50_000_000]} />
          <XAxis label="Compute Units" orientation={"top"} tick={false} axisLine={false} />
          <ReferenceLine y="48000000" stroke="red" />
          <Tooltip content={<CustomTooltip />} />
          <Bar
            dataKey="value.processed"
            stroke="gray"
            fill="gray"
            opacity={1}
            isAnimationActive={false}
          />
          <Bar
            dataKey="value.confirmed"
            stroke="yellow"
            fill="yellow"
            opacity={1}
            isAnimationActive={false}
          />
          <Bar
            dataKey="value.finalized"
            stroke="green"
            fill="green"
            opacity={1}
            isAnimationActive={false}
          />
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
};

export const ExampleAreaTwoBar = () => {
  const slots2 = useWebSocketStore((state) => state.slots2);

  const [type, _setType] = useState<CurveType>("monotone");

  const data = useMemo(() => {
    const entries = Object.entries(slots2);
    const packedData = entries.reduce<PlotInfo[]>((acc, [_groupIdx, slots]) => {
      const chunk = slots.map((elt) => {
        const obj = {
          x: elt.slot,
          y: elt.totalFee,
          commitment: elt.commitment,
          value: {
            confirmed: null,
            finalized: null,
            processed: null,
          },
        } satisfies PlotInfo;
        obj.value[elt.commitment] = obj.y as unknown as null;
        return obj;
      });
      const sorted = chunk.sort((a, b) => a.x - b.x);
      acc.push(...sorted);
      return acc;
    }, []);
    // fill gaps for area plots
    return packedData;
  }, [slots2]);
  return (
    <div
      style={{
        width: "100%",
      }}
    >
      <ResponsiveContainer width="100%" height={200}>
        <ComposedChart
          data={data}
          margin={{
            top: 10,
            right: 10,
            left: 40,
            bottom: 0,
          }}
        >
          <CartesianGrid strokeDasharray="3 3" vertical={false} />
          <YAxis scale="auto" tickFormatter={tickFormatter2} />
          <XAxis label="Earned SOL" orientation={"top"} tick={false} axisLine={false} />
          <Tooltip content={<CustomTooltip2 />} />
          <Bar
            dataKey="value.processed"
            stroke="gray"
            fill="gray"
            opacity={1}
            isAnimationActive={false}
          />
          <Bar
            dataKey="value.confirmed"
            stroke="yellow"
            fill="yellow"
            opacity={1}
            isAnimationActive={false}
          />
          <Bar
            dataKey="value.finalized"
            stroke="green"
            fill="green"
            opacity={1}
            isAnimationActive={false}
          />
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
};

// Drafts for tooltips, better to rewrite
function simpleFormatter(value: string, name: string): [string, string] {
  let formattedValue = value;
  let formattedName = name;

  const newValue = (+value).toLocaleString("en-US", { maximumFractionDigits: 2 });
  if (!Number.isNaN(+value)) formattedValue = newValue;

  if (name === "y") {
    formattedName = `Average fee`;
  }

  return [formattedValue, formattedName];
}

interface PropsAreaThree {
  items: string[];
}

function getColor(arg: string): string {
  const areaColors: Record<string, string> = {
    fee0: "red",
    fee1: "green",
    fee2: "blue",
  };
  return areaColors[arg] || "gray";
}

export const ExampleAreaThree = ({ items }: PropsAreaThree) => {
  const slots2 = useWebSocketStore((state) => state.slots2);

  const [type, _setType] = useState<CurveType>("monotone");

  const data = useMemo(() => {
    const entries = Object.entries(slots2);
    const packedData = entries.reduce<FeesInfo[]>((acc, [_groupIdx, slots]) => {
      const chunk = slots.map((elt) => {
        const obj = {
          x: elt.slot,
          y: elt.feeAverage,
          commitment: elt.commitment,
          fee0: elt.feeLevels[0] || null,
          fee1: elt.feeLevels[1] || null,
          fee2: elt.feeLevels[2] || null,
        } satisfies FeesInfo;
        return obj;
      });
      const sorted = chunk.sort((a, b) => a.x - b.x);
      acc.push(...sorted);
      return acc;
    }, []);
    return packedData;
  }, [slots2]);
  return (
    <div
      style={{
        width: "100%",
      }}
    >
      <ResponsiveContainer width="100%" height={200}>
        <ComposedChart
          data={data}
          margin={{
            top: 10,
            right: 10,
            left: 40,
            bottom: 0,
          }}
        >
          <CartesianGrid strokeDasharray="3 3" vertical={false} />
          <YAxis scale="sqrt" />
          <XAxis label="Fees" orientation={"top"} tick={false} axisLine={false} />
          <Tooltip
            labelFormatter={(label, payload) => {
              if (payload[0]) {
                const slot = payload[0].payload.x;
                return "Slot: " + slot.toLocaleString("en-US");
              }
              return label;
            }}
            formatter={simpleFormatter}
          />
          <Area
            type={type}
            dataKey="y"
            stroke="gray"
            fill="gray"
            opacity={0.7}
            dot={false}
            isAnimationActive={false}
          />
          {items.map((elt) => (
            <Area
              key={elt}
              type={type}
              dataKey={elt}
              stroke={getColor(elt)}
              fill={getColor(elt)}
              opacity={0.99}
              isAnimationActive={false}
            />
          ))}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
};
