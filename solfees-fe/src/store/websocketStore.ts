import { create } from "zustand";

export interface SlotContent {
  commitment: CommitmentStatus;
  feeAverage: number;
  feeLevels: number[];
  hash: string;
  height: number;
  leader: string;
  slot: number;
  time: number;
  totalFee: number;
  totalTransactions: number;
  totalTransactionsFiltered: number;
  totalTransactionsVote: number;
  totalUnitsConsumed: number;
}

export interface StatusUpdate {
  commitment: CommitmentStatus;
  slot: number;
}

export type CommitmentStatus = "processed" | "confirmed" | "finalized";

interface WebSocketState {
  socket: WebSocket | null;
  slots2: Record<string, SlotContent[]>;
  readonlyKeys: string[];
  readwriteKeys: string[];
  percents: number[];
  updateSubscription: () => void;
  updatePercents: (arg: number[]) => void;
  updateReadonlyKeys: (arg: string[]) => void;
  updateReadwriteKeys: (arg: string[]) => void;
  suspendQueue: (arg: boolean) => void;
}

interface ServerAnswerFees {
  result: Array<{
    slot: SlotContent;
  }>;
}

export const useWebSocketStore = create<WebSocketState>((set, get: () => WebSocketState) => {
  const queue: MessageEvent[] = [];
  let lastProcessedTime = Date.now();
  let isLocked = false;

  setInterval(() => {
    if (isLocked) return;
    const lastCommitDuration = Date.now() - lastProcessedTime;
    if (lastCommitDuration > 5_000) {
      /* TODO: make reconnect if no new payloads */
    }
  }, 5000);

  function routeMessages(target: "queue" | "store"): void {
    const socket = get().socket;
    if (!socket) return;

    if (target === "queue") {
      queue.length = 0;
      socket.onmessage = (event) => {
        queue.push(event);
      };
    } else if (target === "store") {
      const handleMessage = (event: MessageEvent) => {
        const data = typeof event.data !== "string" ? event.data : (JSON.parse(event.data) as any);
        if (data.result.slot) {
          const update = data.result.slot as SlotContent;
          {
            const groupIdx = (update.slot / 4) | 0;

            const slots2 = get().slots2;
            if (slots2[groupIdx]) {
              slots2[groupIdx] = [...slots2[groupIdx], update]
                .sort((a, b) => b.slot - a.slot)
                .filter((elt, idx, arr) => {
                  // probably I have dupes because I enqueue a lot of stuff without filtering
                  const sameIdx = arr.findIndex((sameElt) => sameElt.slot === elt.slot);
                  return sameIdx === idx;
                });

              slots2[groupIdx] = [...slots2[groupIdx], update]
                .sort((a, b) => b.slot - a.slot)
                .filter((elt, idx, arr) => {
                  // probably I have dupes because I enqueue a lot of stuff without filtering
                  const sameIdx = arr.findIndex((sameElt) => sameElt.slot === elt.slot);
                  return sameIdx === idx;
                });
            } else {
              slots2[groupIdx] = [update];
              const keys = Object.keys(slots2);
              if (keys.length > 38) {
                const target = Math.min(...keys.map(Number));
                delete slots2[target];
                // TODO if you remove a group -- plots have shift. Probably need split "view" by validator and list of slots for plots
              }
            }

            set({
              slots2,
            });
          }
          return;
        }
        if (data.result.status) {
          const update = data.result.status as StatusUpdate;
          {
            const groupIdx = (update.slot / 4) | 0;

            const slots2 = get().slots2;
            if (!slots2[groupIdx]) {
              // console.warn('no update for groupId:', groupIdx, 'slot:', update.slot, update.commitment)
              return;
            }
            // test

            const newSlots = slots2[groupIdx].map((elt) => {
              if (elt.slot === update.slot) return { ...elt, ...update };
              return elt;
            });
            slots2[groupIdx] = newSlots;

            set({
              slots2: { ...slots2 },
            });
          }
          return;
        }
        if (data.result === "subscribed") {
          return;
        }
        console.warn("unrecognized", data);
      };
      socket.onmessage = function (e: MessageEvent) {
        queue.push(e);
        // perfomance crutch to avoid multiple redraws because we can have up to 30 messages per second
        if (!isLocked && Date.now() - lastProcessedTime > 250) {
          queue.length > 25 && console.log("queued from WS:", queue.length);
          queue.forEach(handleMessage);
          queue.length = 0;
          lastProcessedTime = Date.now();
        }
      };
      queue.forEach(handleMessage);
      queue.length = 0;
    } else {
      console.warn("unknown target", target);
    }
  }

  async function fetchFromHttp(): Promise<void> {
    try {
      // set({slots2: {}})
      const response = await fetch("https://api.solfees.io/api/solfees", {
        method: "POST",
        body: JSON.stringify({
          method: "getRecentPrioritizationFees",
          jsonrpc: "2.0",
          params: [
            {
              readWrite: get().readwriteKeys,
              readOnly: get().readonlyKeys,
              levels: get().percents,
            },
          ],
          id: "1",
        }),
      });
      const serverData = (await response.json()) as ServerAnswerFees;
      // I add messages to the end of the queue. Probably I should add it to the beginning
      serverData.result.forEach((result) => {
        queue.push(new MessageEvent("fromJs", { data: { result } }));
      });
    } catch (error) {
      console.error("Error:", error);
    }
  }

  setTimeout(() => {
    const url = "wss://api.solfees.io/api/solfees/ws";
    const socket = new WebSocket(url);

    socket.onopen = () => {
      set({ socket });
      get().updateSubscription();
    };
    socket.onclose = () => {
      set({
        socket: null,
      });
    };
    socket.onerror = (error) => {
      console.error("WebSocket error:", error);
    };
  }, 125);

  return {
    socket: null,
    slots2: {},
    readonlyKeys: [],
    readwriteKeys: [],
    percents: [2000, 5000, 9000],
    updatePercents: (percents) => {
      set({ percents });
    },
    updateReadonlyKeys: (readonlyKeys) => {
      if (JSON.stringify(readonlyKeys) === JSON.stringify([""])) {
        set({ readonlyKeys: [] });
        return;
      }
      set({ readonlyKeys });
    },
    updateReadwriteKeys: (readwriteKeys) => {
      if (JSON.stringify(readwriteKeys) === JSON.stringify([""])) {
        set({ readwriteKeys: [] });
        return;
      }
      set({ readwriteKeys });
    },
    updateSubscription: () => {
      const socket = get().socket;
      if (socket) {
        const data = {
          id: 0,
          method: "SlotsSubscribe",
          params: {
            readWrite: get().readwriteKeys,
            readOnly: get().readonlyKeys,
            levels: get().percents,
          },
        };
        socket.send(JSON.stringify(data));
        routeMessages("queue");
        fetchFromHttp().then(() => {
          routeMessages("store");
        });
      }
    },
    suspendQueue: (arg) => {
      isLocked = arg;
    },
  };
});
