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
  enabled: boolean;
  status: "active" | "disabled" | "error" | "payment_required" | "quota_exhausted";
  tag_ids: string[];
  binding_count: number;
  cooldown_until: number | null;
  cooldown_code: string | null;
  authorized: boolean;
  authentication_invalid: boolean;
  error_message: string | null;
  error_at: string | null;
  quota: SupplierQuota | null;
  display_name: string | null;
  username: string | null;
  email: string | null;
  plan_type: string | null;
  subscription_expires_at: string | null;
  chatgpt_account_id: string | null;
  chatgpt_user_id: string | null;
  installation_id: string;
  originator: string;
  user_agent: string;
  proxy_id: string | null;
  created_at: string;
  last_used_at: string | null;
  fingerprint: Fingerprint;
  usage?: AccountUsageSummary;
};
export type AccountUsageSummary = {
  lifetime_tokens: number | null;
  peak_daily_tokens: number | null;
  current_streak_days?: number;
  longest_streak_days?: number;
  longest_running_turn_sec?: number | null;
  daily_usage_buckets: { start_date: string; tokens: number | null }[];
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
  local_usage: SupplierCycleUsage | null;
};
export type SupplierCycleUsage = {
  from_ms: number;
  until_ms: number;
  request_count: number;
  cost_nano_usd: number | null;
  tokens: number | null;
  unpriced_requests: number;
  missing_token_requests: number;
};
export type Consumer = {
  user_id: string | null;
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
  description: string;
  sale_price_usd: string | null;
  duration_days: number;
  supplier_tag_id: string | null;
  plan_type: string;
  id: string;
  name: string;
  provider_id: string;
  model_access: "all" | "selected" | "none";
  models: ModelRef[];
  spending_windows: SpendingWindow[];
  allow_purchase: boolean;
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
  max_input_tokens?: number | null;
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
  token_prices: TokenPrice[];
  image_prices: ImagePrice[];
};
export type ModelPreset = {
  provider_id: string;
  model: string;
  kind: "text" | "image";
  version: string;
  source_url: string | null;
  verified_at: string | null;
  token_prices: TokenPrice[];
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
  | {
      status: "complete";
      supplier_id: string;
      reused_existing?: boolean;
      model_sync_error?: string | null;
    };
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
  authenticated_at_ms: number | null;
  requested_at_ms: number | null;
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
  image_usage_json: string | null;
  image_input_usage_json: string | null;
  first_byte_ms: number | null;
  total_ms: number | null;
  requested_at_ms: number;
  status: string;
  http_status: number | null;
  failure_kind: string | null;
  failure_status: number | null;
  error_code: string | null;
  error_message: string | null;
  cost_nano_usd: number | null;
  billing_status: string;
  billing_tier: string | null;
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
  summary: AccountUsageSummary;
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
export type GatewaySettings = {
  ua_mode: "blacklist" | "whitelist";
  ua_rules: string[];
  default_rpm: number;
};
export type PublicUrlSettings = {
  api_url: string;
  user_url: string;
  admin_url: string;
  revision: number;
};
export type SupplierTag = {
  id: string;
  provider_id: string;
  name: string;
  supplier_count: number;
  binding_count: number;
};
export type RpmLimit = { rpm: number | null; default_rpm: number; effective_rpm: number };
export type DesktopSettings = {
  proxy_id: string | null;
  resource_cache_minutes: number;
};
