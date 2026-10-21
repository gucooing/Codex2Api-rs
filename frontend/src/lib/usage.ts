"use client";
import { useIsMobile } from "@/hooks/use-mobile";
import { useErrorToast } from "@/lib/actions";
import { query, type List, type Supplier, type UsagePage, type UsageRecord } from "@/lib/api";
import { useApiKeyLookup } from "@/lib/api-key-lookup";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import { usePageControls } from "@/lib/pagination";
import { usePreference, useSavedFilters, validPageSize } from "@/lib/preferences";
import { useUserLookup } from "@/lib/user-lookup";
import { useId, useState } from "react";

export type SupplierOption = Pick<Supplier, "id" | "display_name" | "email">;

export const emptyFilters = {
  user_id: "",
  user_label: "",
  supplier_label: "",
  key_label: "",
  key_id: "",
  supplier_platform: "",
  supplier_id: "",
  model: "",
  status: "",
  source: "",
  from: "",
  until: "",
};

export function useUsagePageView({ consumerId }: { consumerId?: string }) {
  const fieldId = useId();
  const userLookup = useUserLookup();
  const apiKeyLookup = useApiKeyLookup();
  const {
    filters,
    setFilters,
    applied,
    setApplied,
    ready: preferencesReady,
  } = useSavedFilters(`usage:${consumerId ?? "all"}:filters`, emptyFilters);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = usePreference<number>("usage.page-size", 20, validPageSize);
  const [supplierOpen, setSupplierOpen] = useState(false);
  const [supplierSearch, setSupplierSearch] = useState("");
  const selectedUser = filters.user_id
    ? { id: filters.user_id, username: "", name: filters.user_label }
    : null;
  const selectedSupplier = filters.supplier_id
    ? {
        id: filters.supplier_id,
        display_name: filters.supplier_label || filters.supplier_id,
        email: "",
      }
    : null;
  const selectedApiKey = filters.key_id
    ? {
        id: filters.key_id,
        name: filters.key_label || filters.key_id,
        key_prefix: "",
        username: "",
      }
    : null;
  const suppliers = useResource<List<Supplier>>(
    supplierOpen
      ? `/suppliers/options${query({ search: supplierSearch.trim(), provider_id: filters.supplier_platform, limit: 5 })}`
      : null,
    supplierSearch.trim() ? 250 : 0,
  );
  useErrorToast(suppliers.error);
  const resource = useResource<UsagePage>(
    preferencesReady
      ? `/usage${query({ user_id: consumerId ? "" : applied.user_id, supplier_id: applied.supplier_id, supplier_platform: applied.supplier_platform, key_id: consumerId ? "" : applied.key_id, virtual_account: consumerId ?? "", model: applied.model, status: applied.status, source: applied.source, from: applied.from, until: applied.until, page, page_size: pageSize, tz_offset: new Date().getTimezoneOffset() })}`
      : null,
  );
  const pagination = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    setPage,
    pageSize,
    resource.refreshing,
    setPageSize,
  );
  const update = (key: keyof typeof filters, value: string) =>
    setFilters((current) => ({ ...current, [key]: value }));
  useErrorToast(resource.error);

  return {
    fieldId,
    userLookup,
    apiKeyLookup,
    filters,
    setFilters,
    setApplied,
    setPage,
    supplierOpen,
    setSupplierOpen,
    setSupplierSearch,
    selectedUser,
    selectedSupplier,
    selectedApiKey,
    suppliers,
    resource,
    pagination,
    update,
  } as const;
}

export function useUsageTable() {
  const tableColumns0 = useColumnVisibility(
    "components/usage.tsx:0",
    [
      "消费账户",
      "供应账户",
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

  return { tableColumns0, mobile, selected, setSelected } as const;
}
