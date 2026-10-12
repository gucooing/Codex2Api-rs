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
  enabled: boolean;
  tag_ids: string[];
  allowed_models: string[];
  multiplier: string;
  concurrency_limit: number;
  rpm_limit: number;
  model_rules: ApiModelRule[];
  revision: number | null;
};
export type ApiGroupOption = Pick<ApiGroup, "id" | "name" | "provider_id" | "enabled">;
export type ApiKey = {
  id: string;
  name: string;
  group_id: string;
  group_name: string;
  provider_id: string;
  user_id: string;
  username: string;
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
export type ApiUsage = {
  id: string;
  user_id: string;
  username: string;
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
export const apiTokens = (tokens: number | null) =>
  tokens === null
    ? "—"
    : new Intl.NumberFormat("zh-CN", { notation: "compact", maximumFractionDigits: 2 }).format(
        tokens,
      );
export const apiCost = (nano: number | null) =>
  nano === null ? "—" : `$${(nano / 1e9).toLocaleString("en-US", { maximumFractionDigits: 9 })}`;
export const apiKeyStatus = (key: Pick<ApiKey, "enabled" | "revoked_at">) =>
  key.revoked_at ? "已撤销" : key.enabled ? "已启用" : "已停用";
export const apiUsageStatus = (status: string) =>
  ({
    started: "进行中",
    in_progress: "进行中",
    completed: "已完成",
    failed: "失败",
    client_stopped: "客户端停止",
    interrupted: "已中断",
    incomplete: "未完成",
  })[status] ?? status;
export const apiBillingStatus = (status: string) =>
  ({
    pending: "待结算",
    priced: "已计费",
    settled: "已结算",
    missing_usage: "用量缺失",
    missing_price: "未定价",
    unpriced: "未定价",
    not_charged: "未计费",
    unknown: "未知",
  })[status] ?? status;
