"use client";
import { useIsMobile } from "@/hooks/use-mobile";
import { useErrorToast, toastError } from "@/lib/actions";
import { useColumnVisibility } from "@/lib/columns";
import { usePageControls } from "@/lib/pagination";
import { usePreference } from "@/lib/preferences";
import { useResource } from "@/lib/resource";
import type { UsageRecord, UserUsage } from "@/lib/usage";
import { useId, useState, type FormEvent } from "react";

export const defaults = {
  days: "7",
  model: "",
  status: "all",
  source: "all",
  provider: "all",
  from: "",
  until: "",
};
export function useUsagePage() {
  const [filters, setFilters] = useState(defaults);
  const [applied, setApplied] = useState(defaults);
  const [page, setPage] = useState(1);
  const [size, setSize] = usePreference<number>("usage.page-size", 20, (value) =>
    [10, 20, 30, 50].includes(value),
  );
  const id = useId();
  const params = new URLSearchParams({
    days: applied.days,
    tz_offset: String(new Date().getTimezoneOffset()),
    model: applied.model,
    status: applied.status === "all" ? "" : applied.status,
    provider: applied.provider === "all" ? "" : applied.provider,
    page: String(page),
    limit: String(size),
  });
  if (applied.source !== "all") params.set("source", applied.source);
  if (applied.from) params.set("from_ms", String(new Date(applied.from).getTime()));
  if (applied.until) params.set("until_ms", String(new Date(applied.until).getTime()));
  const resource = useResource<UserUsage>(`/usage?${params}`, 0);
  useErrorToast(resource.error);
  const pagination = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    setPage,
    size,
    !resource.ready,
    setSize,
  );
  const update = (key: keyof typeof defaults, value: string) =>
    setFilters((old) => ({ ...old, [key]: value }));
  const search = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const from = filters.from ? new Date(filters.from).getTime() : undefined;
    const until = filters.until ? new Date(filters.until).getTime() : undefined;
    if (
      (from !== undefined && !Number.isFinite(from)) ||
      (until !== undefined && !Number.isFinite(until)) ||
      (from !== undefined && until !== undefined && from >= until)
    ) {
      toastError("请选择有效的起止时间");
      return;
    }
    setApplied({ ...filters, model: filters.model.trim() });
    setPage(1);
    resource.reload();
  };
  const reset = () => {
    setFilters(defaults);
    setApplied(defaults);
    setPage(1);
    resource.reload();
  };
  return { filters, id, resource, pagination, update, search, reset };
}

export function useUsageTable() {
  const tableColumns0 = useColumnVisibility(
    "usage",
    [
      "消费账户",
      "来源",
      "类型",
      "模型",
      "接口",
      "IP",
      "推理强度 / 速度",
      "用量",
      "费用",
      "耗时",
      "时间 / 状态",
    ],
    ["模型", "接口", "来源", "类型", "用量", "时间 / 状态"],
  );
  const mobile = useIsMobile();
  const [selected, setSelected] = useState<UsageRecord>();
  return { tableColumns0, mobile, selected, setSelected };
}
