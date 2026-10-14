export type UsageCalculation = {
  terms: {
    kind: "cache_write" | "cache_read" | "output" | "input" | "image" | "request";
    quantity: number;
    unit_price_usd: string;
    resolution?: string;
  }[];
  multiplier_micros: number;
  total_nano_usd: number;
};

export function usageFormula(calculation: UsageCalculation | null | undefined) {
  if (!calculation) return "计算明细未记录";
  const labels = { cache_write: "缓写", cache_read: "缓读", output: "输出", input: "输入" };
  const quantity = (value: number) => {
    if (value < 1000) return String(value);
    const unit = value >= 1e6 ? "m" : "k";
    const divisor = value >= 1e6 ? 1e6 : 1e3;
    return `${value / divisor}${unit}`;
  };
  const sum =
    calculation.terms
      .map((term) => {
        if (term.kind === "image") {
          return `图片${term.resolution ? `[${term.resolution}]` : ""}:${term.quantity}张*${term.unit_price_usd}`;
        }
        if (term.kind === "request") return `请求:${term.quantity}次*${term.unit_price_usd}`;
        return `${labels[term.kind]}:${quantity(term.quantity)}*${term.unit_price_usd}`;
      })
      .join(" + ") || "0";
  const expression =
    calculation.multiplier_micros === 1e6 || calculation.terms.length === 0
      ? sum
      : `(${sum})*${calculation.multiplier_micros / 1e6}x`;
  const total = (calculation.total_nano_usd / 1e9).toLocaleString("en-US", {
    maximumFractionDigits: 9,
  });
  return `${expression} = $${total}`;
}
