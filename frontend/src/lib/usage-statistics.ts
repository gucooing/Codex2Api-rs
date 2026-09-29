export type UsageGroup = "hour" | "day" | "model" | "virtual_account";
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
export type UsageStatisticsRow = UsageTotals & { key: string; label: string };
export type UsageStatistics = {
  group_by: UsageGroup;
  from_ms: number;
  until_ms: number;
  tz_offset: number;
  summary: UsageTotals;
  rows: UsageStatisticsRow[];
  model_usage: {
    bucket: string;
    model: string;
    total_tokens: number | null;
    missing_token_requests: number;
  }[];
};
export const usageGroups: Record<UsageGroup, string> = {
  hour: "按小时",
  day: "按天",
  model: "按模型",
  virtual_account: "按虚拟账户",
};
export const percent = (value: number | null | undefined) =>
  value == null ? "-" : `${Number(value.toFixed(1))}%`;
export const usd = (nano: number | null | undefined) =>
  nano == null ? "-" : `$${(nano / 1e9).toLocaleString("en-US", { maximumFractionDigits: 9 })}`;

export function localMinute(date: Date) {
  return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
}

export function statisticsRange(preset: string, now = new Date()) {
  // Include the current minute, with an exclusive upper bound.
  const until = new Date(Math.floor(now.getTime() / 60_000) * 60_000 + 60_000);
  const from = new Date(until);
  if (preset === "today") from.setHours(0, 0, 0, 0);
  else from.setTime(until.getTime() - Number(preset) * 86_400_000);
  return { from: localMinute(from), until: localMinute(until) };
}

export function statisticsRows(data: UsageStatistics | undefined) {
  if (!data) return [];
  if (data.group_by === "model" || data.group_by === "virtual_account") {
    return data.rows
      .map((row) => ({ ...row, label: row.label || "未记录模型" }))
      .sort(
        (a, b) => (b.total_tokens ?? -1) - (a.total_tokens ?? -1) || a.key.localeCompare(b.key),
      );
  }
  if (!data.summary.request_count) return [];
  const step = data.group_by === "hour" ? 3_600_000 : 86_400_000;
  const offset = data.tz_offset * 60_000;
  const rows = new Map(data.rows.map((row) => [row.key, row]));
  const result: UsageStatisticsRow[] = [];
  for (
    let t = Math.floor((data.from_ms - offset) / step) * step;
    t < data.until_ms - offset;
    t += step
  ) {
    const iso = new Date(t).toISOString();
    const key = data.group_by === "hour" ? iso.slice(0, 13) + ":00" : iso.slice(0, 10);
    result.push(
      rows.get(key) ?? {
        key,
        label: key,
        request_count: 0,
        completed_requests: 0,
        failed_requests: 0,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        cached_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        cost_nano_usd: 0,
        cache_rate: null,
        missing_token_requests: 0,
        missing_cache_requests: 0,
        unpriced_requests: 0,
      },
    );
  }
  return result;
}

/** Join all four metrics and model stacks on the same dimension, never on row position. */
export function overviewChart(data: UsageStatistics | undefined) {
  const groupedRows = statisticsRows(data);
  const rows =
    data?.group_by === "model" || data?.group_by === "virtual_account"
      ? groupedRows.slice(0, 20)
      : groupedRows;
  const visibleKeys = new Set(rows.map((row) => row.key));
  const totals = new Map<string, number>();
  const buckets = new Map<string, Map<string, number | null>>();
  for (const row of data?.model_usage ?? []) {
    if (!visibleKeys.has(row.bucket)) continue;
    totals.set(row.model, (totals.get(row.model) ?? 0) + (row.total_tokens ?? 0));
    const bucket = buckets.get(row.bucket) ?? new Map<string, number | null>();
    bucket.set(row.model, row.total_tokens);
    buckets.set(row.bucket, bucket);
  }
  const models = [...totals]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([model], index) => ({
      model,
      key: `model_${index}`,
      label: `模型：${model || "未记录模型"}`,
      color:
        index < 5
          ? `var(--chart-${index + 1})`
          : `color-mix(in oklab, var(--chart-${(index % 5) + 1}) ${Math.round(80 - (40 * Math.floor(index / 5)) / Math.ceil(totals.size / 5))}%, var(--background))`,
    }));
  return {
    models,
    truncated: rows.length < groupedRows.length,
    rows: rows.map((row) => ({
      ...row,
      cost_usd: row.cost_nano_usd == null ? null : row.cost_nano_usd / 1e9,
      ...Object.fromEntries(
        models.map((model) => {
          const bucket = buckets.get(row.key);
          // A missing model in this group consumed zero; reported unknown usage stays NULL.
          return [model.key, bucket?.has(model.model) ? bucket.get(model.model) : 0];
        }),
      ),
    })),
  };
}
