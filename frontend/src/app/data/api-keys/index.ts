"use client";
import { useErrorToast } from "@/lib/actions";
import type { List } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import type { ApiGroupOption, ApiKey } from "@/lib/metered-api";
import { useListResource } from "@/lib/pagination";
import { useId, useState, type FormEvent } from "react";

export function useApiKeysPage() {
  const id = useId();
  const defaults = { search: "", group_id: "", status: "" };
  const [filters, setFilters] = useState(defaults);
  const [applied, setApplied] = useState(defaults);
  const [selected, setSelected] = useState<ApiKey>();
  const groups = useResource<List<ApiGroupOption>>("/api-groups/options");
  const resource = useListResource<ApiKey>("/api-keys", applied);
  const columns = useColumnVisibility(
    "api-keys",
    [
      "用户 / 密钥",
      "API 组",
      "状态",
      "请求数",
      "输入 / 输出",
      "计费金额",
      "当前并发",
      "最后使用",
      "操作",
    ],
    ["用户 / 密钥", "计费金额", "当前并发", "操作"],
  );
  useErrorToast(resource.error);
  useErrorToast(groups.error);
  const search = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setApplied({ ...filters });
    resource.reload(1);
  };
  const reset = () => {
    setFilters(defaults);
    setApplied(defaults);
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
