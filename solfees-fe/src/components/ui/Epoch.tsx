import { useWebSocketStore } from "../../store/websocketStore.ts";
import { useEffect, useMemo } from "react";
import { withTooltip } from "@consta/uikit/withTooltip";
import { Text } from "@consta/uikit/Text";
import { useScheduleStore } from "../../store/scheduleStore.ts";
import { useShallow } from "zustand/react/shallow";

const rtf = new Intl.RelativeTimeFormat("en", {
  localeMatcher: "best fit", // other values: "lookup"
  numeric: "always", // other values: "auto"
  style: "long", // other values: "short" or "narrow"
});

function concatUnit(amount: number, unit: string): string {
  if (!amount) return " ";
  return `${amount} ${unit}${amount % 10 === 1 ? "" : "s"}, `;
}

function formatDuration(seconds: number) {
  const days = Math.floor(seconds / (24 * 3600));
  seconds %= 24 * 3600;
  const hours = Math.floor(seconds / 3600);
  seconds %= 3600;
  const minutes = Math.floor(seconds / 60);
  seconds %= 60;
  seconds |= 0;

  return `${concatUnit(days, "day")}${concatUnit(hours, "hour")}${concatUnit(minutes, "minute")}${concatUnit(seconds, "second")}`
    .trim()
    .replace(/,$/, "");
}

const TextWithTooltip = withTooltip({ content: "Top tooltip" })(Text);

export const Epoch = () => {
  const slots2 = useWebSocketStore((state) => state.slots2);
  const updateSchedule = useScheduleStore(useShallow((state) => state.updateSchedule));

  const lastSlot = useMemo(() => {
    const idx = Math.max(...Object.keys(slots2).map(Number));
    return slots2[idx]?.[0]?.slot || 0;
  }, [slots2]);

  const number = useMemo(() => {
    return (lastSlot / 432_000) | 0;
  }, [lastSlot]);
  const percent = useMemo(() => {
    if (lastSlot) {
      return (((lastSlot - number * 432_000) / 432_000) * 100).toFixed(3) + "%";
    }
    return "";
  }, [lastSlot, number]);
  // TODO probably we should display 00:00:00 countdown with days?
  const humanCountdown = useMemo(() => {
    if (lastSlot) {
      const secs = (432_000 - (lastSlot % 432_000)) * 0.4;
      if (secs < 90) return rtf.format(secs, "seconds"); // 1.5 minutes
      if (secs < 90 * 60) return rtf.format(Math.ceil(secs / 60), "minutes"); // 1.5 hours
      if (secs < 1.5 * 86_400) return rtf.format(Math.ceil(secs / 3_600), "hours"); // 1.5 days
      return rtf.format(Math.ceil(secs / 86_400), "days");
    }
    return "...";
  }, [lastSlot]);
  const tooltipForHumanCountdown = useMemo(() => {
    if (lastSlot) {
      const secs = (432_000 - (lastSlot % 432_000)) * 0.4;
      return formatDuration(secs) + ". Based on 400ms/slot.";
    }
    return "...";
  }, [lastSlot]);

  useEffect(() => {
    updateSchedule(lastSlot).then(void 0);
  }, [lastSlot]);

  return (
    <div className="flex-col justify-start items-end gap-1 inline-flex">
      <div className="self-stretch justify-between items-end inline-flex">
        <div className="justify-start items-end gap-1 flex">
          <div className="text-center text-[#002033] text-sm font-normal font-['Inter'] leading-[21px]">
            Epoch
          </div>
          <div className="text-center text-[#09d288] text-xs font-normal font-['Inter'] leading-[18px]">
            {number || "..."}
          </div>
        </div>
        <div className="text-center text-[#002033] text-[10px] font-normal font-['Inter'] leading-[15px]">
          <TextWithTooltip
            tooltipProps={{
              content: tooltipForHumanCountdown,
              direction: "downCenter",
            }}
          >
            {humanCountdown}
          </TextWithTooltip>
        </div>
      </div>
      <div className="w-[200px] h-[3px] relative overflow-hidden">
        <div className="w-full h-[3px] left-0 top-0 absolute bg-[#004166]/20 rounded"></div>
        <div
          style={{ width: percent }}
          className="h-[3px] left-0 top-0 absolute bg-[#09d288] rounded"
        ></div>
      </div>
      <div className="text-[#002033]/60 text-[10px] font-normal font-['Inter'] leading-[15px]">
        {percent}
      </div>
    </div>
  );
};
