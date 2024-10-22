export const isProduction = import.meta.env.MODE === "production";

export const percentFromStore = (num?: number): number => {
  if (num) {
    const value = num / 100;
    return value;
  }
  return 0;
}
