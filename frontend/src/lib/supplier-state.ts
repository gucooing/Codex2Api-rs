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
