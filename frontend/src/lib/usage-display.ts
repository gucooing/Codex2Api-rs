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
