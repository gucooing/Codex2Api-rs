export const date = (value?: string | number | null) =>
  value == null || value === ""
    ? "—"
    : new Date(typeof value === "number" && value < 1e12 ? value * 1000 : value).toLocaleString(
        "zh-CN",
      );
export const money = (value?: string | number | null) =>
  value == null
    ? "未计价"
    : `$${Number(value).toLocaleString("en-US", { maximumFractionDigits: 9 })}`;
