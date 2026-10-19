"use client";
import { modelMarketPricing, type ModelMarketItem } from "./pricing";
import { useErrorToast } from "@/lib/actions";
import type { ApiGroups, ApiModelMarket } from "@/lib/api-access";
import { usePageControls } from "@/lib/pagination";
import { useResource } from "@/lib/resource";
import { useId, useState, type FormEvent } from "react";

export function useModelMarketPage() {
  const id = useId();
  const [groupId, setGroupId] = useState("");
  const [filters, setFilters] = useState({ search: "", platform: "" });
  const [applied, setApplied] = useState(filters);
  const [page, setPage] = useState(1);
  const [size, setSize] = useState(20);
  const [selected, setSelected] = useState<ModelMarketItem>();
  const groups = useResource<ApiGroups>("/api-groups", 0);
  const params = new URLSearchParams({
    ...applied,
    group_id: groupId,
    page: String(page),
    page_size: String(size),
  });
  const resource = useResource<ApiModelMarket>(`/model-market?${params}`, 0);
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
  const selectGroup = (value: string) => {
    setGroupId(value === "base" ? "" : value);
    setPage(1);
    setSelected(undefined);
  };
  const search = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setApplied({ ...filters });
    setPage(1);
    resource.reload();
  };
  const reset = () => {
    const empty = { search: "", platform: "" };
    setFilters(empty);
    setApplied(empty);
    setPage(1);
    resource.reload();
  };
  const reload = () => {
    groups.reload();
    resource.reload();
  };
  const items: ModelMarketItem[] = (resource.data?.items ?? []).map((model) => ({
    model,
    pricing: modelMarketPricing(model, !!groupId),
  }));
  return {
    id,
    groups,
    groupId,
    currentGroup: resource.data?.group,
    showOtherBilling:
      (!applied.search.trim() ||
        "chatgpt openai 搜索 search".includes(applied.search.trim().toLowerCase())) &&
      (!applied.platform || applied.platform === "chatgpt"),
    searchPrice: resource.data?.operations.find(
      (price) => price.provider_id === "chatgpt" && price.operation === "search",
    ),
    selectGroup,
    filters,
    setFilters,
    resource,
    items,
    pagination,
    selected,
    setSelected,
    search,
    reset,
    reload,
  };
}
