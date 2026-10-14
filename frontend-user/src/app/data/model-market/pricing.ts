import type { ApiCatalogModel, ApiTokenPrice } from "@/lib/metered-api";

type InputRange = Pick<ApiTokenPrice, "min_input_tokens" | "max_input_tokens">;
export type ModelTokenPrice = InputRange & {
  tier: string;
  standardMultiplier: string | null;
  input: string;
  output: string;
  cached: string;
  cacheWrite: string;
};
export type ModelImagePrice = { resolution: string; price: string };
export type ModelServicePrice = {
  tier: string;
  multiplier: string | null;
  followsStandard: boolean;
  ranges: InputRange[];
  prices: ModelTokenPrice[];
};
export type ModelMarketPricing = {
  standard: ModelTokenPrice[];
  services: ModelServicePrice[];
  images: ModelImagePrice[];
  startingToken: ModelTokenPrice | undefined;
  startingImage: ModelImagePrice | undefined;
};
export type ModelMarketItem = { model: ApiCatalogModel; pricing: ModelMarketPricing };

const tiers = ["standard", "fast", "flex", "ultrafast"];
const tierOrder = (tier: string) => {
  const index = tiers.indexOf(tier);
  return index === -1 ? tiers.length : index;
};
function coveredRanges(prices: InputRange[]): InputRange[] {
  const ranges: InputRange[] = [];
  for (const price of prices) {
    const last = ranges.at(-1);
    if (
      last &&
      last.max_input_tokens != null &&
      last.max_input_tokens + 1 === price.min_input_tokens
    ) {
      last.max_input_tokens = price.max_input_tokens;
    } else {
      ranges.push({
        min_input_tokens: price.min_input_tokens,
        max_input_tokens: price.max_input_tokens,
      });
    }
  }
  return ranges;
}

export function modelMarketPricing(model: ApiCatalogModel, grouped: boolean): ModelMarketPricing {
  const tokens = model.prices
    .map((price) => ({
      tier: price.tier,
      min_input_tokens: price.min_input_tokens,
      max_input_tokens: price.max_input_tokens,
      standardMultiplier: price.standard_multiplier,
      input: grouped ? price.input_usd_per_million : price.base_input_usd_per_million,
      output: grouped ? price.output_usd_per_million : price.base_output_usd_per_million,
      cached: grouped ? price.cached_usd_per_million : price.base_cached_usd_per_million,
      cacheWrite: grouped
        ? price.cache_write_usd_per_million
        : price.base_cache_write_usd_per_million,
    }))
    .sort(
      (a, b) =>
        tierOrder(a.tier) - tierOrder(b.tier) ||
        a.tier.localeCompare(b.tier) ||
        a.min_input_tokens - b.min_input_tokens,
    );
  const standard = tokens.filter((price) => price.tier === "standard");
  const services = [...new Set(tokens.map((price) => price.tier))]
    .filter((tier) => tier !== "standard")
    .map((tier) => {
      const prices = tokens.filter((price) => price.tier === tier);
      const multiplier = prices[0].standardMultiplier ?? null;
      const ranges = coveredRanges(prices);
      const followsStandard = JSON.stringify(ranges) === JSON.stringify(coveredRanges(standard));
      return {
        tier,
        multiplier:
          followsStandard &&
          multiplier != null &&
          prices.every((price) => price.standardMultiplier === multiplier)
            ? multiplier
            : null,
        followsStandard,
        ranges,
        prices,
      };
    });
  const images = model.image_prices.map((price) => ({
    resolution: price.resolution,
    price: grouped ? price.price_usd : price.base_price_usd,
  }));
  const startingImage = images.reduce<ModelImagePrice | undefined>(
    (lowest, price) => (!lowest || Number(price.price) < Number(lowest.price) ? price : lowest),
    undefined,
  );
  return { standard, services, images, startingToken: standard[0] ?? tokens[0], startingImage };
}
