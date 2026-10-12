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
  provider_id: string;
  multiplier: string;
  concurrency_limit: number;
  rpm_limit: number;
  models_count: number;
};
export type ApiGroups = { items: ApiGroup[]; api_base_url: string };
export type ApiTokenPrice = {
  tier: string;
  min_input_tokens: number;
  max_input_tokens: number | null;
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
  provider_id: string;
  model: string;
  kind: "text" | "image";
  multiplier: string;
  group_concurrency_limit: number;
  group_rpm_limit: number;
  concurrency_limit: number;
  rpm_limit: number;
  prices: ApiTokenPrice[];
  image_prices: ApiImagePrice[];
};
export type ApiModelMarket = ListPage<ApiCatalogModel> & { group: ApiGroup | null };
export type ApiKey = {
  id: string;
  name: string;
  group_id: string;
  group_name: string;
  provider_id: string;
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
export type ApiUsage = {
  id: string;
  key_id: string;
  key_name: string;
  key_prefix: string;
  group_id: string;
  group_name: string;
  provider_id: string;
  endpoint: string;
  model: string;
  actual_model: string | null;
  multiplier: string;
  input_tokens: number | null;
  output_tokens: number | null;
  cached_tokens: number | null;
  cache_write_tokens: number | null;
  reasoning_tokens: number | null;
  image_count: number | null;
  image_size: string | null;
  requested_at_ms: number;
  completed_at_ms: number | null;
  status: string;
  http_status: number | null;
  cost_nano_usd: number | null;
  billing_status: string;
  wallet_debit_cents: number | null;
};
export type ApiUsagePage = ListPage<ApiUsage> & {
  summary: {
    requests: number;
    input_tokens: number | null;
    output_tokens: number | null;
    cost_nano_usd: number | null;
    unknown_cost_requests: number;
  };
  wallet_pending_nano_usd: number;
};
export const apiPlatform = (provider: string) =>
  provider === "chatgpt" ? "ChatGPT" : provider === "grok" ? "Grok" : provider;
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
    ? `${price.min_input_tokens.toLocaleString()}+`
    : `${price.min_input_tokens.toLocaleString()}–${price.max_input_tokens.toLocaleString()}`;
