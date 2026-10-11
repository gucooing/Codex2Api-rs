"use client";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import { useErrorToast } from "@/lib/actions";
import type { SupplierQuota } from "@/lib/api";
import { type Json } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useState } from "react";

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
