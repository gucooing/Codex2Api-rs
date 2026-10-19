"use client";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import { useErrorToast } from "@/lib/actions";
import { type Json } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useState } from "react";

const text = (value: Json | undefined) =>
  typeof value === "string" || typeof value === "number" ? String(value) : undefined;
const record = (value: Json | undefined) =>
  value && typeof value === "object" && !Array.isArray(value) ? value : undefined;
const tierLabel = (tier: Json | undefined) => {
  const entry = record(tier);
  return text(entry?.name) ?? text(entry?.id) ?? text(tier);
};

/** One quota: a model with its own, or the models sharing a pool (server grouped). */
type ModelQuota = {
  key: string;
  name: string;
  models: string[];
  upstream_models: string[];
  remaining_fraction: number | null;
  reset_at: number | null;
};

/** Official details (loadCodeAssist) and the per-model quota read (fetchAvailableModels). */
export function useAntigravityOfficialData({
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
    observed_at?: string;
    refresh_error?: string;
    models?: ModelQuota[] | null;
  }>(`/suppliers/${id}/official?section=${section}${refresh ? `&refresh=true&r=${refresh}` : ""}`);
  useErrorToast(resource.error);
  useErrorToast(resource.data?.refresh_error);
  const value = record(resource.data?.value);
  const allowed = value?.allowedTiers;
  const fields: [string, string | undefined][] = [
    ["项目", text(value?.cloudaicompanionProject)],
    ["当前档位", tierLabel(value?.currentTier)],
    ["付费档位", tierLabel(value?.paidTier)],
    [
      "可用档位",
      Array.isArray(allowed) ? allowed.map(tierLabel).filter(Boolean).join("、") : undefined,
    ],
  ];
  const models = (resource.data?.models ?? [])
    .map((item) => ({
      key: item.key,
      model: item.models.join(" · "),
      name: item.name,
      usedPercent:
        item.remaining_fraction == null
          ? null
          : Math.min(100, Math.max(0, (1 - item.remaining_fraction) * 100)),
      resetAt: item.reset_at,
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
  return { setRefresh, now, resource, fields, models } as const;
}
