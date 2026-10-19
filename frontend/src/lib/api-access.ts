import { providerLabel } from "@/lib/providers";
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
  search_price_usd: string | null;
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
export const apiPlatform = providerLabel;
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
