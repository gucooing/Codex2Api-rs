import type { ListPage } from "./pagination";

export type ApiModelRule = {
  model: string;
  multiplier: string | null;
  concurrency_limit: number;
  rpm_limit: number;
};
export type ApiGroup = {
  id: string;
  name: string;
  description: string;
  multiplier: string;
  search_price_usd: string | null;
  concurrency_limit: number;
  rpm_limit: number;
  models_count: number;
};
export type ApiGroups = { items: ApiGroup[]; api_base_url: string };
export type ApiTokenPrice = {
  tier: string;
  min_input_tokens: number;
  max_input_tokens: number | null;
  standard_multiplier: string | null;
  input_usd_per_million: string;
  cached_usd_per_million: string;
  cache_write_usd_per_million: string;
  output_usd_per_million: string;
  base_input_usd_per_million: string;
  base_cached_usd_per_million: string;
  base_cache_write_usd_per_million: string;
  base_output_usd_per_million: string;
};
export type ApiImagePrice = { resolution: string; base_price_usd: string; price_usd: string };
export type ApiCatalogModel = {
  platform: string;
  model: string;
  kind: "text" | "image";
  multiplier: string;
  group_concurrency_limit: number;
  group_rpm_limit: number;
  concurrency_limit: number;
  rpm_limit: number;
  prices: ApiTokenPrice[];
  image_prices: ApiImagePrice[];
  search_price: string | null;
};
export type ApiOperationPrice = {
  provider_id: string;
  operation: string;
  price_usd: string | null;
};
export type ApiModelMarket = ListPage<ApiCatalogModel> & {
  group: ApiGroup | null;
  operations: ApiOperationPrice[];
};
export type ApiKey = {
  id: string;
  name: string;
  group_id: string;
  group_name: string;
  key_prefix: string;
  enabled: boolean;
  revoked_at: string | null;
  created_at: string;
  last_used_at: string | null;
  revision: number;
  total_requests: number;
  input_tokens: number | null;
  output_tokens: number | null;
  cost_nano_usd: number | null;
  unknown_cost_requests: number;
  active_requests: number;
};
export type CreatedApiKey = { key: ApiKey; secret: string };
const platformNames: Record<string, string> = {
  chatgpt: "ChatGPT",
  grok: "Grok",
  antigravity: "Antigravity",
};
export const apiPlatform = (provider: string) => platformNames[provider] ?? provider;
export const apiLimit = (limit: number) => (limit === 0 ? "无限" : String(limit));
export const apiPrice = (usd: string | null | undefined) =>
  usd == null || usd === "" ? "待定价" : `$${usd}`;
export const apiCost = (nano: number | null | undefined) =>
  nano == null ? "—" : `$${(nano / 1e9).toLocaleString("en-US", { maximumFractionDigits: 9 })}`;
export const apiDate = (value: string | number | null | undefined) =>
  value == null ? "—" : new Date(value).toLocaleString();
export const apiKeyStatus = (key: Pick<ApiKey, "enabled" | "revoked_at">) =>
  key.revoked_at ? "已撤销" : key.enabled ? "已启用" : "已停用";
export const apiTier = (tier: string) =>
  ({ standard: "标准", fast: "Fast", flex: "Flex", ultrafast: "Ultrafast" })[tier] ?? tier;
export const apiRange = (price: Pick<ApiTokenPrice, "min_input_tokens" | "max_input_tokens">) =>
  price.max_input_tokens == null
    ? price.min_input_tokens === 0
      ? "不限"
      : `≥ ${price.min_input_tokens.toLocaleString()}`
    : `${price.min_input_tokens.toLocaleString()}–${price.max_input_tokens.toLocaleString()}`;
