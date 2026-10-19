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
  const resource = useResource<{ value: Json; observed_at?: string; refresh_error?: string }>(
    `/suppliers/${id}/official?section=${section}${refresh ? `&refresh=true&r=${refresh}` : ""}`,
  );
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
  const models = Object.entries(record(value?.models) ?? {})
    .map(([model, info]) => {
      const entry = record(info);
      const quota = record(entry?.quotaInfo);
      const remaining =
        typeof quota?.remainingFraction === "number" ? quota.remainingFraction : null;
      const reset = text(quota?.resetTime);
      const resetAt = reset ? Date.parse(reset) / 1000 : Number.NaN;
      return {
        model,
        name: text(entry?.displayName) ?? model,
        usedPercent: remaining == null ? null : Math.min(100, Math.max(0, (1 - remaining) * 100)),
        resetAt: Number.isFinite(resetAt) ? resetAt : null,
      };
    })
    .sort((a, b) => a.name.localeCompare(b.name));
  return { setRefresh, now, resource, fields, models } as const;
}
