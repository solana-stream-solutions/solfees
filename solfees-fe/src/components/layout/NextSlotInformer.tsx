import {useWebSocketStore} from "../../store/websocketStore.ts";
import {useScheduleStore} from "../../store/scheduleStore.ts";
import {useShallow} from "zustand/react/shallow";
import {useMemo} from "react";

export const NextSlotInformer = () => {
  const slots2 = useWebSocketStore(useShallow(state => state.slots2));
  const indices = useScheduleStore(useShallow(state => state.indices));
  const leaders = useScheduleStore(useShallow(state => state.leaders));

  const lastSlot = useMemo(() => {
    const idx = Math.max(...Object.keys(slots2).map(Number));
    return slots2[idx]?.[0]?.slot || 0;
  }, [slots2])

  const groupIndex = indices[lastSlot % 432_000] || 0;
  const nextLeader = leaders[groupIndex] || ''
  const nextSlot = (((lastSlot / 4)|0)+1)*4

  if(!lastSlot) return null;
  return <div className="w-full flex flex-nowrap justify-center gap-4">
    <div>{nextLeader}</div>
    <div>{nextSlot}</div>
  </div>
}