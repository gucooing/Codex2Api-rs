"use client";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import { useActions, useErrorToast } from "@/lib/actions";
import type { SupplierQuota } from "@/lib/api";
import { request, type Json, type List } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useListResource } from "@/lib/pagination";
import { useState } from "react";
import { toast } from "sonner";

export type GrokModel = {
  id: string;
  model: string;
  name?: string;
  context_window?: number;
  api_backend: string;
};

export function useGrokModelCatalog({ id }: { id: string }) {
  const resource = useListResource<GrokModel, { observed_at: string | null; stale: boolean }>(
    `/suppliers/grok/${encodeURIComponent(id)}/models`,
  );
  const pagination = resource.pagination;
  const actions = useActions();
  useErrorToast(resource.error);
  const handleClick = () =>
    void actions.run("grok-catalog", async () => {
      await request<Json>(`/suppliers/grok/${encodeURIComponent(id)}/models`, {
        method: "POST",
      });
      resource.reload();
    });
  return { resource, pagination, actions, handleClick } as const;
}

export function useGrokModelSync({ onSynced }: { onSynced: () => void }) {
  const actions = useActions();
  const key = "grok-model-sync";
  const handleClick = () =>
    void actions.run(
      key,
      async () => {
        const result = await request<List<{ success: boolean; models?: number }>>(
          "/models/grok/sync",
          { method: "POST" },
        );
        onSynced();
        const failed = result.items.filter((i) => !i.success).length;
        if (failed) toast.error(`${failed} 个 Grok 账户同步失败，其余目录已更新`);
        else toast.success("Grok 真实模型目录与可用价格预设已同步");
      },
      { success: "" },
    );
  return { actions, key, handleClick } as const;
}

export function useGrokOfficialData({
  id,
  section,
}: {
  id: string;
  section: string;
  onUsername?: (username: string | undefined) => void;
  onUpdated?: () => void;
}) {
  const [refresh, setRefresh] = useState(0);
  const now = useQuotaClock();
  const resource = useResource<{
    value: Json;
    quota?: SupplierQuota;
    observed_at?: string;
    refresh_error?: string;
  }>(`/suppliers/${id}/official?section=${section}${refresh ? `&refresh=true&r=${refresh}` : ""}`);
  useErrorToast(resource.error);
  useErrorToast(resource.data?.refresh_error);
  const fields =
    section === "details"
      ? [
          ["用户编号", ["userId"]],
          ["邮箱", ["email"]],
          ["名字", ["firstName"]],
          ["姓氏", ["lastName"]],
          ["身份类型", ["principalType"]],
          ["团队编号", ["teamId"]],
          ["团队名称", ["teamName"]],
          ["订阅", ["subscriptionTierDisplay"]],
        ]
      : section === "usage"
        ? [
            ["订阅", ["subscription_tier_display"]],
            ["Grok Build 权限", ["allow_access"]],
            ["按需使用", ["on_demand_enabled"]],
            ["服务提示", ["gate_message"]],
          ]
        : [
            ["已用比例", ["config", "creditUsagePercent"]],
            ["周期开始", ["config", "currentPeriod", "start"]],
            ["周期结束", ["config", "currentPeriod", "end"]],
            ["预付余额（美分）", ["config", "prepaidBalance", "val"]],
            ["按需已用（美分）", ["config", "onDemandUsed", "val"]],
          ];

  return { setRefresh, now, resource, fields } as const;
}
