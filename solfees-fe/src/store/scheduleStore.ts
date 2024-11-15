import { create } from "zustand";

interface ServerAnswerSchedule {
  result: null | EpochSchedule;
}

interface EpochSchedule {
  indices: number[];
  leaders: string[];
}

type ScheduleState = {
  updateSchedule: (epoch: number) => Promise<void>;
  indices: number[];
  leaders: string[];
  epoch: number;
};

export const useScheduleStore = create<ScheduleState>((set, get: () => ScheduleState) => {
  let isRequested = false;
  let requestFailedAt = new Date(Date.now() - 9999);

  async function fetchSchedule(epoch: number): Promise<null | EpochSchedule> {
    if (!epoch) return null;
    if (isRequested) return null;
    if (requestFailedAt && Date.now() - requestFailedAt.getTime() > 5_000) {
      try {
        isRequested = true;
        const response = await fetch("https://api.solfees.io/api/solfees", {
          method: "POST",
          body: JSON.stringify({
            method: "getLeaderSchedule",
            jsonrpc: "2.0",
            params: [{ epoch }],
            id: "1",
          }),
        });
        const serverData = (await response.json()) as ServerAnswerSchedule;
        if (serverData.result === null) {
          requestFailedAt = new Date();
          return null;
        }
        return serverData.result;
      } catch (e) {
        console.warn(e);
        requestFailedAt = new Date();
      } finally {
        isRequested = false;
      }
    }
    return null;
  }

  async function updateSchedule(slot: number) {
    const epoch = (slot / 432_000) | 0;
    if (epoch && epoch !== get().epoch) {
      const dataFromLS = loadFromLS(epoch);
      const dataFromApi = dataFromLS ? null : await fetchSchedule(epoch);
      const data = dataFromLS || dataFromApi;
      if (data) {
        saveToLS(epoch, data);
        set({
          epoch,
          indices: data.indices,
          leaders: data.leaders,
        });
      }
    }
  }

  function loadFromLS(requestedEpoch: number): null | EpochSchedule {
    const epochFromLS = localStorage.getItem("savedEpoch");
    if (epochFromLS && +epochFromLS === requestedEpoch) {
      const dataFromLS = localStorage.getItem("savedSchedule");
      if (dataFromLS) {
        const schedule = JSON.parse(dataFromLS) as EpochSchedule;
        return schedule;
      }
    }
    return null;
  }

  function saveToLS(epoch: number, schedule: EpochSchedule) {
    localStorage.setItem("savedEpoch", epoch.toString());
    localStorage.setItem("savedSchedule", JSON.stringify(schedule));
  }

  return {
    epoch: 0,
    indices: [],
    leaders: [],
    updateSchedule,
  };
});
