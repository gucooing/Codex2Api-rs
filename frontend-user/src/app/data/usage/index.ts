"use client";
import { useErrorToast } from "@/lib/actions";
import { usePageControls } from "@/lib/pagination";
import { useResource } from "@/lib/resource";
import { tokens, usageCost, type UserUsage } from "@/lib/usage";
import { useId, useState } from "react";

export const defaults = { days: "7", model: "", status: "all", provider: "all" };

export function useUsagePage() {
  const [filters, setFilters] = useState(defaults);
  const [applied, setApplied] = useState(defaults);
  const [page, setPage] = useState(1);
  const [size, setSize] = useState(20);
  const id = useId();
  const resource = useResource<UserUsage>(
    `/usage?days=${applied.days}&tz_offset=${new Date().getTimezoneOffset()}&model=${encodeURIComponent(applied.model)}&status=${applied.status === "all" ? "" : applied.status}&provider=${applied.provider === "all" ? "" : applied.provider}&page=${page}&limit=${size}`,
    0,
  );
  const pagination = usePageControls(
    page,
    resource.data?.total,
    setPage,
    size,
    !resource.data,
    setSize,
  );
  const summary = resource.data?.summary;
  useErrorToast(resource.error);
  const update = (key: keyof typeof defaults, value: string) =>
    setFilters((old) => ({ ...old, [key]: value }));
  const summaries = [
    ["请求数", summary?.request_count?.toLocaleString() ?? "—"],
    ["输入 / 输出 Token", `${tokens(summary?.input_tokens)} / ${tokens(summary?.output_tokens)}`],
    ["缓存命中 Token", tokens(summary?.cached_tokens)],
    ["已计费用量（USD）", usageCost(summary?.cost_nano_usd)],
  ];

  return {
    filters,
    setFilters,
    setApplied,
    setPage,
    id,
    resource,
    pagination,
    summary,
    update,
    summaries,
  } as const;
}
