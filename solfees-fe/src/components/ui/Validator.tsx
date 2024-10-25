interface Props {
  leader: string;
}
export const Validator = ({ leader }: Props) => {
  return (
    <div className="px-3 w-full h-full flex flex-col flex-nowrap justify-center">
      <h1 className="font-bold">Validator Name:</h1>
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
