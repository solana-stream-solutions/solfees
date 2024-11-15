import { CustomRow } from "../../common/prepareValidatorRow.ts";
import { SlotWithCopy } from "./SlotWithCopy.tsx";

interface Props {
  leader: string;
  slots: CustomRow["slots"];
}
export const Validator = ({ leader, slots }: Props) => {
  const isNext = slots.some((elt) => elt.commitment === "next-leader");
  if (isNext)
    return (
      <div className="px-3 py-3 flex flex-col flex-nowrap">
        <div className="flex flex-col ml-auto mr-auto items-start">
          <div className="flex flex-row flex-nowrap w-full justify-between gap-2">
            <h1 className="font-bold">Next Validator Name:</h1>
            <a
              className="hover:underline"
              href={`https://www.validators.app/validators/${leader}?locale=en&network=mainnet`}
              target="_blank"
            >
              {leader.slice(0, 5)}…{leader.slice(-4)}
            </a>
          </div>
          <div className="flex flex-row flex-nowrap w-full justify-between gap-2">
            <h1 className="font-bold">Next slot:</h1>
            {slots[0] ? <SlotWithCopy value={slots[0].slot} /> : "unknown"}
          </div>
        </div>
      </div>
    );
  return (
    <div className="px-3 w-full h-full flex flex-col flex-nowrap justify-center">
      <h1 className="font-bold">{isNext ? "Next" : ""} Validator Name:</h1>
      <a
        className="hover:underline"
        href={`https://www.validators.app/validators/${leader}?locale=en&network=mainnet`}
        target="_blank"
      >
        {leader.slice(0, 5)}…{leader.slice(-4)}
      </a>
    </div>
  );
};
