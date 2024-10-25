import { CustomRow } from "../../common/prepareValidatorRow.ts";

interface Props {
  leader: string;
  slots: CustomRow["slots"];
}
export const Validator = ({ leader, slots }: Props) => {
  const isNext = slots.some((elt) => elt.commitment === "next-leader");
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
