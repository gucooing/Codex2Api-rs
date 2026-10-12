"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import type { ApiGroups, ApiUsage, ApiUsagePage } from "@/lib/metered-api";
import { usePageControls } from "@/lib/pagination";
import { useResource } from "@/lib/resource";
import { useId, useState, type FormEvent } from "react";

export function useApiUsagePage() {
  const id = useId();
  const actions = useActions();
  const defaults = { search: "", group_id: "", status: "", from: "", until: "" };
  const [filters, setFilters] = useState(defaults);
  const [applied, setApplied] = useState({
    search: "",
    group_id: "",
    status: "",
    from_ms: "",
    until_ms: "",
  });
  const [page, setPage] = useState(1);
  const [size, setSize] = useState(20);
  const [selected, setSelected] = useState<ApiUsage>();
  const groups = useResource<ApiGroups>("/api-groups", 0);
  const params = new URLSearchParams({ page: String(page), page_size: String(size) });
  for (const [key, value] of Object.entries(applied)) if (value) params.set(key, value);
  const resource = useResource<ApiUsagePage>(`/api-usage?${params}`, 0);
  const pagination = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    setPage,
    size,
    !resource.ready,
    setSize,
  );
  useErrorToast(groups.error);
  useErrorToast(resource.error);
  const search = (event: FormEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "api-usage-search",
      async () => {
        const from_ms = filters.from ? new Date(filters.from).getTime() : null;
        const until_ms = filters.until ? new Date(filters.until).getTime() : null;
        if (
          (from_ms !== null && !Number.isFinite(from_ms)) ||
          (until_ms !== null && !Number.isFinite(until_ms))
        )
          throw new Error("请输入有效的查询时间");
        if (from_ms !== null && until_ms !== null && from_ms >= until_ms)
          throw new Error("结束时间须晚于开始时间");
        setApplied({
          search: filters.search,
          group_id: filters.group_id,
          status: filters.status,
          from_ms: from_ms === null ? "" : String(from_ms),
          until_ms: until_ms === null ? "" : String(until_ms),
        });
        setPage(1);
        resource.reload();
      },
      "",
    );
  const reset = () => {
    setFilters(defaults);
    setApplied({ search: "", group_id: "", status: "", from_ms: "", until_ms: "" });
    setPage(1);
    resource.reload();
  };
  return {
    id,
    filters,
    setFilters,
    groups,
    resource,
    pagination,
    selected,
    setSelected,
    search,
    reset,
  };
}
