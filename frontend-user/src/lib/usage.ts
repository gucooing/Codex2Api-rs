export type UsageTotals = {
  request_count: number;
  completed_requests: number;
  failed_requests: number;
  input_tokens: number | null;
  output_tokens: number | null;
  total_tokens: number | null;
  cached_tokens: number | null;
  cache_write_tokens: number | null;
  reasoning_tokens: number | null;
  cost_nano_usd: number | null;
  cache_rate: number | null;
  missing_token_requests: number;
  missing_cache_requests: number;
  unpriced_requests: number;
};
export type UsageRow = UsageTotals & { key: string; label: string };
export type UsageRecord = {
  id: string;
  consumer_name: string;
  subject_kind: string;
  reasoning_effort: string | null;
  service_tier: string | null;
  image_usage_json: string | null;
  image_input_usage_json: string | null;
  key_id: string | null;
  source_name: string | null;
  endpoint: string;
  upstream_endpoint: string | null;
  client_ip: string | null;
  transport: string;
  stream: boolean | null;
  error_code: string | null;
  error_reason: string | null;
  multiplier_micros: number;
  wallet_debit_cents: number;
  image_count: number | null;
  image_size: string | null;
  provider_id: string;
  model: string | null;
  actual_model: string | null;
  requested_at_ms: number;
  status: string;
  input_tokens: number | null;
  output_tokens: number | null;
  cached_tokens: number | null;
  cache_write_tokens: number | null;
  reasoning_tokens: number | null;
  total_ms: number | null;
  first_token_ms: number | null;
  cost_nano_usd: number | null;
  billing_status: string;
  billing_calculation: import("./usage-calculation").UsageCalculation | null;
};
export type UserUsage = {
  wallet_pending_nano_usd: number;
  from_ms: number;
  until_ms: number;
  tz_offset: number;
  summary: UsageTotals;
  rows: UsageRow[];
  items: UsageRecord[];
  total: number;
  page: number;
  limit: number;
};
export const tokens = (value: number | null | undefined) =>
  value == null
    ? "—"
    : new Intl.NumberFormat("zh-CN", { notation: "compact", maximumFractionDigits: 2 }).format(
        value,
      );
export const usageCost = (nano: number | null | undefined) =>
  nano == null ? "—" : `$${(nano / 1e9).toFixed(6)}`;
export function usageSeries(data: UserUsage | undefined) {
  if (!data) return [];
  const rows = new Map(data.rows.map((row) => [row.key, row]));
  const result = [];
  for (let at = data.from_ms; at < data.until_ms; at += 86400000) {
    const key = new Date(at - data.tz_offset * 60000).toISOString().slice(0, 10);
    const row = rows.get(key);
    result.push({
      date: key,
      request_count: row?.request_count ?? 0,
      failed_requests: row?.failed_requests ?? 0,
      total_tokens: row ? row.total_tokens : 0,
      cost_usd: row ? (row.cost_nano_usd === null ? null : row.cost_nano_usd / 1e9) : 0,
      missing_token_requests: row?.missing_token_requests ?? 0,
      unpriced_requests: row?.unpriced_requests ?? 0,
    });
  }
  return result;
}
