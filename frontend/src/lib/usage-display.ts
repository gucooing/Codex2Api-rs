import type { UsageRecord } from "./api";

export const failedUsage = (record: Pick<UsageRecord, "status">) =>
  ["failed", "incomplete", "interrupted"].includes(record.status);

export function tokenCount(value: number | null | undefined) {
  if (value == null) return "-";
  if (value < 1000) return value.toLocaleString("en-US");
  return new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 }).format(
    value,
  );
}

type ImageUsage = { resolution: string | null; count: number };
function parseImageUsage(value: string | null | undefined): ImageUsage[] {
  if (!value) return [];
  try {
    const parsed: unknown = JSON.parse(value);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (item): item is ImageUsage =>
        Boolean(item) &&
        typeof item === "object" &&
        typeof (item as { count?: unknown }).count === "number",
    );
  } catch {
    return [];
  }
}
export function imageUsageLabel(record: {
  image_count: number | null;
  image_usage_json: string | null;
  image_input_usage_json: string | null;
  endpoint: string;
  image_size: string | null;
}) {
  const output = parseImageUsage(record.image_usage_json);
  const input = parseImageUsage(record.image_input_usage_json);
  const format = (items: ImageUsage[]) =>
    items.length
      ? items.map((item) => `${item.count}张 ${item.resolution ?? "分辨率未知"}`).join("、")
      : record.image_size || "分辨率未知";
  const outputText = format(output);
  if (record.endpoint.endsWith("/images/edits") && input.length) {
    return `上传 ${format(input)}；回复 ${outputText}`;
  }
  return `${record.image_count ?? 0}张（${outputText}）`;
}
export function cacheRate(record: Pick<UsageRecord, "input_tokens" | "cached_tokens">) {
  if (record.input_tokens == null || record.input_tokens <= 0 || record.cached_tokens == null)
    return "-";
  return `${Number(((record.cached_tokens / record.input_tokens) * 100).toFixed(1))}%`;
}
export function duration(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value) || value < 0) return "-";
  if (value < 1000) return `${Math.round(value)} ms`;
  if (value < 60000) return `${Number((value / 1000).toFixed(2))}秒`;
  const seconds = Math.floor(value / 1000);
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}分${seconds % 60 ? `${seconds % 60}秒` : ""}`;
  return `${Math.floor(minutes / 60)}小时${minutes % 60 ? `${minutes % 60}分` : ""}`;
}
export function usageFailure(record: UsageRecord) {
  return (
    record.error_message ||
    record.error_code ||
    (record.http_status != null && record.http_status >= 400
      ? `上游返回 HTTP ${record.http_status}`
      : "此历史记录未保存失败原因")
  );
}

export const usageStatuses = [
  {
    value: "in_progress",
    label: "进行中",
    className: "border-blue-600/20 bg-blue-600/10 text-blue-700 dark:text-blue-400",
  },
  {
    value: "completed",
    label: "成功",
    className: "border-green-600/20 bg-green-600/10 text-green-700 dark:text-green-400",
  },
  {
    value: "failed",
    label: "失败",
    className: "border-red-600/20 bg-red-600/10 text-red-700 dark:text-red-400",
  },
] as const;

export function usageStatus(status: string) {
  const value = failedUsage({ status })
    ? "failed"
    : status === "client_stopped"
      ? "completed"
      : status;
  return (
    usageStatuses.find((item) => item.value === value) ?? {
      label: status || "未知",
      className: "text-muted-foreground",
    }
  );
}
