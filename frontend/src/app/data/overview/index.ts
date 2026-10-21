"use client";
import { type ChartConfig } from "@/components/ui/chart";
import { toastError, useErrorToast } from "@/lib/actions";
import { query, type List, type Supplier } from "@/lib/api";
import { useApiKeyLookup } from "@/lib/api-key-lookup";
import { useResource } from "@/lib/hooks";
import { usePreference, useSavedFilters } from "@/lib/preferences";
import {
  overviewChart,
  statisticsRange,
  usageGroups,
  type UsageGroup,
  type UsageStatistics,
} from "@/lib/usage-statistics";
import { useMemo, useState } from "react";

export const defaults = {
  key_id: "",
  key_label: "",
  supplier_platform: "",
  supplier_id: "",
  supplier_label: "",
  model: "",
  status: "",
  group_by: "day" as UsageGroup,
  preset: "7",
  from: "",
  until: "",
};

export const chartConfig = {
  total_tokens: { label: "Token 用量", color: "var(--chart-1)" },
  request_count: { label: "请求数", color: "var(--chart-2)" },
  cache_rate: { label: "缓存率", color: "var(--chart-3)" },
  cost_usd: { label: "费用（USD）", color: "var(--chart-4)" },
};

export const defaultVisibility = {
  models: true,
  total_tokens: true,
  request_count: true,
  cache_rate: true,
  cost_usd: true,
};

export function useOverviewStatistics() {
  const {
    filters,
    setFilters,
    applied: saved,
    setApplied,
    ready: preferencesReady,
  } = useSavedFilters(
    "overview.filters",
    defaults,
    (value) =>
      ["today", "1", "7", "30", "custom"].includes(value.preset) &&
      Object.hasOwn(usageGroups, value.group_by),
  );
  const applied = useMemo(
    () => ({ ...saved, ...(saved.preset === "custom" ? {} : statisticsRange(saved.preset)) }),
    [saved],
  );
  const [visible, setVisible] = usePreference("overview.series", defaultVisibility);
  const apiKeyLookup = useApiKeyLookup();
  const [supplierOpen, setSupplierOpen] = useState(false);
  const [supplierSearch, setSupplierSearch] = useState("");
  const selectedApiKey = filters.key_id
    ? {
        id: filters.key_id,
        name: filters.key_label || filters.key_id,
        key_prefix: "",
        username: "",
      }
    : null;
  const selectedSupplier = filters.supplier_id
    ? { id: filters.supplier_id, display_name: filters.supplier_label, email: "" }
    : null;
  const suppliers = useResource<List<Supplier>>(
    supplierOpen
      ? `/suppliers/options${query({ search: supplierSearch.trim(), provider_id: filters.supplier_platform, limit: 5 })}`
      : null,
    supplierSearch.trim() ? 250 : 0,
  );
  const path = `/overview/usage${query({ key_id: applied.key_id, supplier_platform: applied.supplier_platform, supplier_id: applied.supplier_id, model: applied.model, status: applied.status, group_by: applied.group_by, from: applied.from, until: applied.until, tz_offset: new Date().getTimezoneOffset() })}`;
  const resource = useResource<UsageStatistics>(preferencesReady ? path : null);
  useErrorToast(resource.error);
  useErrorToast(suppliers.error);
  const update = (key: keyof typeof filters, value: string) =>
    setFilters((current) => ({ ...current, [key]: value }));
  const apply = (values: typeof filters) => {
    const range =
      values.preset === "custom"
        ? { from: values.from, until: values.until }
        : statisticsRange(values.preset);
    const span = new Date(range.until).getTime() - new Date(range.from).getTime();
    const maxDays = values.group_by === "hour" ? 31 : 366;
    if (!Number.isFinite(span) || span <= 0 || span > maxDays * 86_400_000) {
      toastError(`请选择有效时间范围，结束时间需晚于开始时间，当前分组最多查询 ${maxDays} 天`);
      return;
    }
    setApplied(
      values.preset === "custom" ? { ...values, ...range } : { ...values, from: "", until: "" },
    );
    resource.reload();
  };
  const summary = resource.data?.summary;
  const timeSeries = applied.group_by === "day" || applied.group_by === "hour";
  const chart = overviewChart(resource.data);
  const config: ChartConfig = {
    ...chartConfig,
    ...Object.fromEntries(
      chart.models.map((model) => [model.key, { label: model.label, color: model.color }]),
    ),
  };

  return {
    filters,
    setFilters,
    applied,
    visible,
    setVisible,
    apiKeyLookup,
    supplierOpen,
    setSupplierOpen,
    setSupplierSearch,
    selectedApiKey,
    selectedSupplier,
    suppliers,
    resource,
    update,
    apply,
    summary,
    timeSeries,
    chart,
    config,
  } as const;
}

export function useOverview() {
  const { data, error, reload } = useResource<{
    supplier_count: number;
    consumer_count: number;
    normal_consumer_count: number;
    user_count: number;
    active_user_count: number;
    models_count: number;
  }>(`/overview?tz_offset=${new Date().getTimezoneOffset()}`);
  useErrorToast(error);

  return { data, reload } as const;
}
