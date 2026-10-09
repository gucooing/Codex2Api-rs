"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type Fingerprint, type Json, type SupplierQuota } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import { useListResource } from "@/lib/pagination";
import { duration, tokenCount } from "@/lib/usage-display";
import { useEffect, useId, useState } from "react";

export function useChatgptFingerprintFields({
  value,
  onChange,
}: {
  value: Fingerprint;
  onChange: (value: Fingerprint) => void;
}) {
  const fieldId = useId();
  const actions = useActions();
  const handleClick = () =>
    void actions.run(
      "components\\suppliers.tsx:action:11",
      async () => {
        const result = await request<{ timezone: string }>(
          `/proxies/${value.proxy_id}/check/timezone`,
          { method: "POST", body: {} },
        );
        onChange({ ...value, timezone: result.timezone });
      },
      { confirm: undefined, danger: false, success: undefined },
    );
  return { fieldId, actions, handleClick } as const;
}

export function useChatgptOfficialData({
  id,
  section,
  onUpdated,
}: {
  id: string;
  section: string;
  onUpdated?: () => void;
}) {
  const [refresh, setRefresh] = useState(0);
  const resource = useResource<{
    value: Json;
    quota?: SupplierQuota;
    observed_at?: string;
    refresh_error?: string | null;
    routing?: {
      status: "not_observed" | "ready" | "stale" | "invalid";
      backend_origin: string | null;
      constraint: "NO_CONSTRAINT" | "us" | "us_cr" | null;
      message: string | null;
    };
  }>(`/suppliers/${id}/official?section=${section}${refresh ? `&refresh=true&r=${refresh}` : ""}`);
  const root = resource.data?.value;
  const value = root && typeof root === "object" && !Array.isArray(root) ? root : {};
  useErrorToast(resource.error);
  useErrorToast(resource.data?.refresh_error ?? undefined);
  useEffect(() => {
    if (section === "quota" && resource.data) onUpdated?.();
  }, [section, resource.data, onUpdated]);

  return { setRefresh, resource, value } as const;
}

export function useChatgptPluginRecords({ path }: { path: string }) {
  const resource = useListResource<Json>(path, { section: "$.plugins" });
  const pagination = resource.pagination;
  const rows = pagination.rows;
  useErrorToast(resource.error);

  return { resource, pagination, rows } as const;
}

export function useOfficialCredits({ id, onRefresh }: { id: string; onRefresh: () => void }) {
  const tableColumns2 = useColumnVisibility(
    "components/suppliers.tsx:2",
    ["名称", "类型", "状态", "到期时间", "操作"],
    ["名称", "状态", "操作"],
  );
  const actions = useActions();
  const resource = useListResource<{ [key: string]: Json }>(
    `/suppliers/chatgpt/${id}/official/rows`,
    { kind: "credits" },
  );
  const pagination = resource.pagination;
  const credits = pagination.rows;
  useErrorToast(resource.error);
  const consume = async (creditId?: string) => {
    await request(`/suppliers/${id}/credits/consume`, {
      method: "POST",
      body: creditId ? { credit_id: creditId } : {},
    });
    onRefresh();
  };
  const handleClick = (credit: { [key: string]: Json }) =>
    void actions.run("components\\suppliers.tsx:action:20", () => consume(String(credit.id)), {
      confirm: "使用此供应账户的一次官方重置额度？",
      danger: false,
      success: undefined,
    });
  const handleClick2 = () =>
    void actions.run("components\\suppliers.tsx:action:21", () => consume(), {
      confirm: "使用此供应账户的一次官方重置额度？",
      danger: false,
      success: undefined,
    });
  return { tableColumns2, actions, pagination, credits, handleClick, handleClick2 } as const;
}

export function useOfficialFields({
  value,
  section,
  id,
}: {
  value: Json;
  section: string;
  id: string;
}) {
  const tableColumns1 = useColumnVisibility(
    "components/suppliers.tsx:1",
    ["账户", "类型", "订阅"],
    ["账户", "类型", "订阅"],
  );
  const root = value && typeof value === "object" && !Array.isArray(value) ? value : {};
  const profile =
    root.profile && typeof root.profile === "object" && !Array.isArray(root.profile)
      ? root.profile
      : root;
  const statsValue =
    root.stats && typeof root.stats === "object" && !Array.isArray(root.stats)
      ? root.stats
      : profile.stats && typeof profile.stats === "object" && !Array.isArray(profile.stats)
        ? profile.stats
        : {};
  const stats =
    statsValue.stats && typeof statsValue.stats === "object" && !Array.isArray(statsValue.stats)
      ? statsValue.stats
      : statsValue;
  const resource = useListResource<Json>(
    section === "details" ? `/suppliers/chatgpt/${id}/official/rows` : null,
    { kind: "details" },
  );
  const pagination = resource.pagination;
  const rows = pagination.rows;
  useErrorToast(resource.error);
  const days = Array.isArray(stats.daily_usage_buckets) ? stats.daily_usage_buckets : [];
  const fields =
    section === "usage"
      ? [
          {
            label: "累计 Token",
            value: tokenCount(
              typeof stats.lifetime_tokens === "number" ? stats.lifetime_tokens : null,
            ),
          },
          {
            label: "单日最高 Token",
            value: tokenCount(
              typeof stats.peak_daily_tokens === "number" ? stats.peak_daily_tokens : null,
            ),
          },
          {
            label: "当前连续使用天数",
            value:
              typeof stats.current_streak_days === "number"
                ? `${stats.current_streak_days}天`
                : "—",
          },
          {
            label: "最长连续使用天数",
            value:
              typeof stats.longest_streak_days === "number"
                ? `${stats.longest_streak_days}天`
                : "—",
          },
          {
            label: "最长任务时长",
            value:
              typeof stats.longest_running_turn_sec === "number"
                ? duration(stats.longest_running_turn_sec * 1000)
                : "—",
          },
        ]
      : [{ label: "默认账户", value: root.default_account_id }];

  return { tableColumns1, pagination, rows, days, fields } as const;
}

export type SearchPrice = { price: string | null; revision: number | null };

export function useSearchBilling() {
  const id = useId();
  const resource = useResource<SearchPrice>("/billing/chatgpt/search");
  const [draft, setDraft] = useState<SearchPrice>();
  const actions = useActions();
  const busy = actions.isBusy("search-price");
  useErrorToast(resource.error);
  const price = draft?.price ?? resource.data?.price ?? "";
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!resource.ready || !draft || busy) return;
    void actions.run(
      "search-price",
      async () => {
        await request<SearchPrice>("/billing/chatgpt/search", {
          method: "PUT",
          body: { ...draft, price: price.trim() || null },
        });
        setDraft(undefined);
        resource.reload();
      },
      { success: "搜索价格已保存" },
    );
  };
  return { id, resource, draft, setDraft, busy, price, handleSubmit } as const;
}
