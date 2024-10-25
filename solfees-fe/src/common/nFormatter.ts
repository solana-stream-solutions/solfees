export function nFormatter(num: number, digits = 0, preserveZeros = false): string {
  const lookup = [
    {
      value: 1,
      symbol: "",
    },
    {
      value: 1e3,
      symbol: "k",
    },
    {
      value: 1e6,
      symbol: "M",
    },
    {
      value: 1e9,
      symbol: "G",
    },
    {
      value: 1e12,
      symbol: "T",
    },
    {
      value: 1e15,
      symbol: "P",
    },
    {
      value: 1e18,
      symbol: "E",
    },
  ];
  const regexp = /\.0+$|(?<=\.[0-9]*[1-9])0+$/;
  const item = lookup
    .slice()
    .reverse()
    .find((item) => num >= item.value);
  if (item) {
    const divided = (num / item.value).toFixed(digits);
    const truncated = preserveZeros ? divided : divided.replace(regexp, "");
    return truncated.concat(item.symbol);
  }
  return "0";
}
