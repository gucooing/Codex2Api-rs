"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import type { List } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import type { ApiGroupOption, ApiUsage, ApiUsagePage } from "@/lib/metered-api";
import { useListResource } from "@/lib/pagination";
import { useId, useState, type FormEvent } from "react";

export function useApiUsagePage() {
  const id = useId();
  const actions = useActions();
  const defaults = { search: "", group_id: "", status: "", from: "", until: "" };
  const [filters, setFilters] = useState(defaults);
  const [applied, setApplied] = useState<{
    search: string;
    group_id: string;
    status: string;
    from_ms: number | null;
    until_ms: number | null;
  }>({ search: "", group_id: "", status: "", from_ms: null, until_ms: null });
  const [selected, setSelected] = useState<ApiUsage>();
  const groups = useResource<List<ApiGroupOption>>("/api-groups/options");
  const resource = useListResource<
    ApiUsage,
    Pick<ApiUsagePage, "summary" | "wallet_pending_nano_usd">
  >("/api-usage", applied);
  const columns = useColumnVisibility(
    "api-usage",
    ["时间", "用户 / 密钥", "API 组", "模型 / 接口", "状态", "输入 / 输出", "倍率 / 费用", "操作"],
    ["模型 / 接口", "状态", "倍率 / 费用", "操作"],
  );
  useErrorToast(resource.error);
  useErrorToast(groups.error);
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
          from_ms,
          until_ms,
        });
        resource.reload(1);
      },
      "",
    );
  const reset = () => {
    setFilters(defaults);
    setApplied({ search: "", group_id: "", status: "", from_ms: null, until_ms: null });
    resource.reload(1);
  };
  return {
    id,
    filters,
    setFilters,
    groups,
    resource,
    pagination: resource.pagination,
    columns,
    selected,
    setSelected,
    search,
    reset,
  };
}
