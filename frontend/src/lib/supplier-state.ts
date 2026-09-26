import type { SupplierQuotaWindow } from "./api";

export const supplierStatusLabel = (status: "active" | "disabled" | "error") =>
  ({ active: "启用", disabled: "停用", error: "错误" })[status];

export function quotaWindowLabel(window: SupplierQuotaWindow): string {
  const seconds = window.limit_window_seconds;
  if (seconds == null || !Number.isFinite(seconds) || seconds <= 0)
    return window.id === "primary_window" ? "主额度" : "次额度";
  if (seconds % 86400 === 0) return `${seconds / 86400}天`;
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  if (seconds % 60 === 0) return `${seconds / 60}分钟`;
  return `${seconds}秒`;
}

export function quotaResetLabel(resetAt: number | null | undefined, now: number): string {
  if (resetAt == null || !Number.isFinite(resetAt)) return "重置时间未提供";
  const remaining = resetAt * 1000 - now;
  if (remaining <= 0) return "已到重置时间，待更新";
  const minutes = Math.ceil(remaining / 60000);
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  const rest = minutes % 60;
  return `${days ? `${days}天 ` : ""}${hours ? `${hours}小时 ` : ""}${!days && rest ? `${rest}分钟` : ""}`.trim();
}
export const percentLabel = (used: number) => `${Number(used.toFixed(1))}%`;

/** The ledger stores settled prices; display never reprices historical usage. */
export function cycleUsageLabel(window: SupplierQuotaWindow): string {
  const usage = window.local_usage;
  if (!usage) return "用量待确认";
  const amount = (value: number, scale: number, prefix: string, suffix: string) => {
    const scaled = value / scale;
    return scaled > 0 && scaled < 0.01
      ? `<${prefix}0.01${suffix}`
      : `${prefix}${scaled.toFixed(2)}${suffix}`;
  };
  const cost =
    usage.cost_nano_usd == null
      ? "$未知"
      : `${amount(usage.cost_nano_usd, 1e9, "$", "")}${usage.unpriced_requests ? "+?" : ""}`;
  const tokens =
    usage.tokens == null
      ? "Token 未知"
      : `${amount(usage.tokens, 1e6, "", "M")}${usage.missing_token_requests ? "+?" : ""}`;
  return `${cost} / ${tokens}`;
}

export function cycleUsageTitle(window: SupplierQuotaWindow): string {
  const usage = window.local_usage;
  if (!usage) return "官方未提供完整周期边界，暂不能统计本地用量";
  const details = [
    `本地周期：${new Date(usage.from_ms).toLocaleString()} — ${new Date(usage.until_ms).toLocaleString()}`,
    `${usage.request_count} 次请求；按请求开始时间归属周期，金额使用已结算账本，1M = 1,000,000 Token`,
  ];
  if (usage.unpriced_requests)
    details.push(`${usage.unpriced_requests} 次请求金额未确定，已知金额仅为小计`);
  if (usage.missing_token_requests)
    details.push(`${usage.missing_token_requests} 次请求 Token 不完整，已知 Token 仅为小计`);
  return details.join("；");
}
