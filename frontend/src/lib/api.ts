export { request, query, ApiError } from "./http";

export type List<T> = { items: T[] };
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type ModelRef = { provider_id: string; model: string };
export type Fingerprint = {
  os_type: string;
  os_version: string;
  arch: string;
  terminal: string;
  proxy_id: string | null;
  timezone: string;
};
export type Supplier = {
  id: string;
  provider_id: string;
  status: "active" | "disabled" | "error";
  authorized: boolean;
  error_message: string | null;
  error_at: string | null;
  quota: SupplierQuota | null;
  display_name: string | null;
  username: string | null;
  email: string | null;
  plan_type: string | null;
  chatgpt_account_id: string | null;
  installation_id: string;
  originator: string;
  user_agent: string;
  proxy_id: string | null;
  created_at: string;
  last_used_at: string | null;
  fingerprint: Fingerprint;
};
export type SupplierQuota = {
  observed_at: string;
  stale: boolean;
  windows: SupplierQuotaWindow[];
};
export type SupplierQuotaWindow = {
  id: "primary_window" | "secondary_window";
  limit_window_seconds: number | null;
  used_percent: number | null;
  reset_at: number | null;
};
export type Consumer = {
  id: string;
  username: string;
  name: string;
  email: string;
  provider_id: string;
  plan_id: string;
  plan_name: string;
  subscription_status: "active" | "expired" | "free";
  plan_type: string;
  subscription_expires_at: string | null;
  enabled: boolean;
  created_at: string;
  effective_plan: string;
};
export type ConsumerWrite = Pick<
  Consumer,
  "username" | "name" | "email" | "provider_id" | "plan_id" | "subscription_expires_at" | "enabled"
> & { password: string };
export type Plan = {
  id: string;
  name: string;
  provider_id: string;
  model_access: "all" | "selected";
  models: ModelRef[];
  free_model_access: "none" | "all" | "selected";
  free_models: ModelRef[];
  free_access_enabled: boolean;
  primary_cost_limit_usd: string | null;
  weekly_cost_limit_usd: string | null;
  free_primary_cost_limit_usd: string | null;
  free_weekly_cost_limit_usd: string | null;
  spending_windows: SpendingWindow[];
  free_spending_windows: SpendingWindow[];
  enabled: boolean;
  revision: number;
  updated_at_ms: number;
};
export type SpendingWindow = {
  duration_seconds: 18000 | 604800 | 2592000;
  cost_limit_usd: string | null;
};
export type Plans = List<Plan>;
export type TokenPrice = {
  tier: "standard" | "fast" | "flex";
  min_input_tokens: number;
  input_rate: string;
  cached_rate: string;
  cache_write_rate: string;
  output_rate: string;
};
export type ImagePrice = { resolution: string; price: string };
export type Model = {
  provider_id: string;
  model: string;
  kind: "text" | "image";
  enabled: boolean;
  revision: number | null;
  codex_metadata_status?: "verified" | "unavailable" | "not_applicable";
  codex_metadata_source?: string | null;
  token_prices: TokenPrice[];
  image_prices: ImagePrice[];
};
export type Proxy = {
  id: string;
  name: string;
  protocol: "http" | "https" | "socks5" | "socks5h";
  host: string;
  port: number;
  username: string | null;
  has_password: boolean;
  display_url: string;
  account_count: number;
  exit_ip: string | null;
  country: string | null;
  region: string | null;
  city: string | null;
  timezone: string | null;
  connection_ok: boolean | null;
  connection_latency_ms: number | null;
  connection_error: string | null;
  connection_checked_at: string | null;
  quality_ok: boolean | null;
  quality_latency_ms: number | null;
  quality_http_status: number | null;
  quality_error: string | null;
  quality_checked_at: string | null;
};
export type ProxyWrite = Pick<Proxy, "name" | "protocol" | "host" | "port" | "username"> & {
  password: string | null;
};
export type OAuth =
  | {
      status: "pending";
      method: string;
      state: string;
      authorize_url?: string;
      verification_url?: string;
      user_code?: string;
      interval?: number;
    }
  | { status: "complete"; supplier_id: string };
export type BusinessField = {
  path: string[];
  label: string;
  group: string;
  kind: "boolean" | "select" | "text" | "date" | "url" | "integer" | "string";
  description: string;
  choices: [string, string][];
  optional: boolean;
};
export type Config = {
  key: string;
  label: string;
  description: string;
  readonly: boolean;
  value: Json;
  revision: number;
  fields: BusinessField[];
  write_origin?: string;
  updated_at_ms?: number;
};
export type Device = {
  id: string;
  user_agent: string;
  installation_id: string | null;
  scopes: string;
  created_at: string;
  last_login_at: string;
  last_used_at: string | null;
};
export type UsageRecord = {
  id: string;
  account_id: string;
  account_name: string;
  subject_id: string;
  subject_name: string;
  subject_kind: string;
  provider_id: string;
  endpoint: string;
  transport: string;
  model: string | null;
  actual_model: string | null;
  upstream_request_id: string | null;
  reasoning_effort: string | null;
  service_tier: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
  cached_tokens: number | null;
  cache_write_tokens: number | null;
  reasoning_tokens: number | null;
  image_size: string | null;
  image_count: number | null;
  first_byte_ms: number | null;
  total_ms: number | null;
  requested_at_ms: number;
  status: string;
  http_status: number | null;
  error_code: string | null;
  error_message: string | null;
  cost_nano_usd: number | null;
  billing_status: string;
};
export type UsagePage = {
  records: UsageRecord[];
  total: number;
  page: number;
  page_size: number;
};
export type QuotaWindow = {
  used_percent: number;
  limit_window_seconds: number;
  reset_at: number;
  used_usd: string;
  limit_usd: string;
  remaining_usd: string;
};
export type AccountUsage = {
  summary: {
    lifetime_tokens: number | null;
    peak_daily_tokens: number | null;
    daily_usage_buckets: { start_date: string; tokens: number | null }[];
  };
  quota: {
    plan_type: string;
    rate_limit: {
      allowed: boolean;
      primary_window: QuotaWindow | null;
      secondary_window: QuotaWindow | null;
    };
    billing: {
      used_usd: string;
      pending_requests: number;
      unpriced_requests: number;
      legacy_requests: number;
    };
  };
};
export type GatewaySettings = { ua_mode: "blacklist" | "whitelist"; ua_rules: string[] };
export type DesktopSettings = {
  proxy_id: string | null;
  collect_diagnostics: boolean;
  resource_cache_minutes: number;
};
