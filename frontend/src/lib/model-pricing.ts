import type { Model, ModelPreset, TokenPrice } from "./api";

export function findModelPreset(
  presets: ModelPreset[] | undefined,
  provider: string,
  model: string,
) {
  return presets?.find(
    (preset) => preset.provider_id === provider && preset.model === model.trim(),
  );
}

export function withModelPreset(model: Model, preset: ModelPreset): Model {
  if (
    !preset.token_prices.length ||
    model.provider_id !== preset.provider_id ||
    model.model.trim() !== preset.model
  ) {
    throw new Error("该模型没有完整预设价格。");
  }
  return {
    ...model,
    model: preset.model,
    kind: preset.kind,
    token_prices: preset.token_prices.map((price) => ({ ...price })),
    image_prices: [],
  };
}

export const priceFields = [
  ["input_rate", "普通输入"],
  ["output_rate", "输出"],
  ["cached_rate", "缓存读取"],
  ["cache_write_rate", "缓存写入"],
] as const;
export type BasePrice = Omit<TokenPrice, "tier">;
type Tier = { mode: "off" | "multiplier" | "custom"; multiplier: string; rows: TokenPrice[] };
export const additionalPriceTiers = ["fast", "flex", "ultrafast"] as const;
export const priceTierLabels = {
  standard: "标准",
  fast: "Fast",
  flex: "Flex",
  ultrafast: "Ultrafast",
};
export type PricingDraft = { ranges: BasePrice[] } & Record<
  (typeof additionalPriceTiers)[number],
  Tier
>;
export const emptyBasePrice = (start = 0): BasePrice => ({
  min_input_tokens: start,
  input_rate: "",
  output_rate: "",
  cached_rate: "",
  cache_write_rate: "",
});
const unit = 1_000_000n;
function decimal(value: string): bigint {
  if (!/^\d+(\.\d{1,6})?$/.test(value.trim()))
    throw new Error("价格和倍率须为非负数字，最多六位小数。");
  const [whole, fraction = ""] = value.trim().split(".");
  return BigInt(whole) * unit + BigInt(fraction.padEnd(6, "0"));
}
function format(value: bigint): string {
  return `${value / unit}.${(value % unit).toString().padStart(6, "0")}`.replace(/\.?0+$/, "");
}
export function multiplyPrice(price: string, multiplier: string): string {
  const product = decimal(price) * decimal(multiplier);
  if (product % unit !== 0n) throw new Error("倍率计算后的单价超过六位小数，请调整价格或倍率。");
  const result = product / unit;
  if (result > 9_223_372_036_854_775_807n) throw new Error("倍率计算后的单价过大。");
  return format(result);
}
export function pricingDraft(prices: TokenPrice[]): PricingDraft {
  const ranges = prices
    .filter((row) => row.tier === "standard")
    .sort((a, b) => a.min_input_tokens - b.min_input_tokens)
    .map(({ tier: _tier, ...row }) => {
      void _tier;
      return row;
    });
  const tier = (name: (typeof additionalPriceTiers)[number]): Tier => {
    const rows = prices.filter((row) => row.tier === name).map((row) => ({ ...row }));
    if (!rows.length) return { mode: "off", multiplier: "1", rows };
    let ratio: bigint | undefined;
    try {
      if (rows.length !== ranges.length) throw new Error("different intervals");
      for (const row of rows) {
        const base = ranges.find((r) => r.min_input_tokens === row.min_input_tokens);
        if (!base) throw new Error("different intervals");
        if ((base.max_input_tokens ?? null) !== (row.max_input_tokens ?? null))
          throw new Error("different upper bounds");
        for (const [key] of priceFields) {
          const a = decimal(base[key]),
            b = decimal(row[key]);
          if (a === 0n) {
            if (b !== 0n) throw new Error("non-proportional");
            continue;
          }
          if ((b * unit) % a !== 0n) throw new Error("non-terminating ratio");
          const next = (b * unit) / a;
          if (ratio !== undefined && next !== ratio) throw new Error("non-proportional");
          ratio = next;
        }
      }
      return { mode: "multiplier", multiplier: format(ratio ?? unit), rows };
    } catch {
      // Existing non-proportional tier prices remain explicit and unchanged.
      return { mode: "custom", multiplier: "1", rows };
    }
  };
  return { ranges, fast: tier("fast"), flex: tier("flex"), ultrafast: tier("ultrafast") };
}
export function pricingRows(draft: PricingDraft): TokenPrice[] {
  const starts = draft.ranges.map((row) => row.min_input_tokens);
  if (
    starts.length &&
    (starts[0] !== 0 ||
      starts.some(
        (v, i) =>
          !Number.isSafeInteger(v) || v < 0 || v > 10_000_000 || (i > 0 && v <= starts[i - 1]),
      ))
  ) {
    throw new Error("上下文区间起点须递增，基础区间从 0 开始，最大为 10000000 Token。");
  }
  const result: TokenPrice[] = draft.ranges.map((row) => ({ ...row, tier: "standard" }));
  for (const name of additionalPriceTiers) {
    const tier = draft[name];
    if (tier.mode === "off") continue;
    if (tier.mode === "custom") {
      result.push(...tier.rows);
      continue;
    }
    if (!draft.ranges.length) throw new Error("请先添加基础价格。");
    result.push(
      ...draft.ranges.map((row): TokenPrice => ({
        tier: name,
        min_input_tokens: row.min_input_tokens,
        ...(row.max_input_tokens != null ? { max_input_tokens: row.max_input_tokens } : {}),
        input_rate: multiplyPrice(row.input_rate, tier.multiplier),
        output_rate: multiplyPrice(row.output_rate, tier.multiplier),
        cached_rate: multiplyPrice(row.cached_rate, tier.multiplier),
        cache_write_rate: multiplyPrice(row.cache_write_rate, tier.multiplier),
      })),
    );
  }
  for (const row of result) {
    if (
      row.max_input_tokens != null &&
      (!Number.isSafeInteger(row.max_input_tokens) ||
        row.max_input_tokens < row.min_input_tokens ||
        row.max_input_tokens > 10_000_000)
    )
      throw new Error("上下文上限须不小于起点，且不超过 10000000 Token。");
    for (const [key] of priceFields) decimal(row[key]);
  }
  return result;
}
