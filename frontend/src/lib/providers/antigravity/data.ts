"use client";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type Fingerprint, type Json, type SupplierQuotaGroup } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useEffect, useId } from "react";

const text = (value: Json | undefined) =>
  typeof value === "string" || typeof value === "number" ? String(value) : undefined;
const record = (value: Json | undefined) =>
  value && typeof value === "object" && !Array.isArray(value) ? value : undefined;
const tierLabel = (tier: Json | undefined) => {
  const entry = record(tier);
  return text(entry?.name) ?? text(entry?.id) ?? text(tier);
};

/** Fingerprint form: the selected proxy's detected exit timezone fills the field. */
export function useAntigravityFingerprintFields({
  value,
  onChange,
}: {
  value: Fingerprint;
  onChange: (value: Fingerprint) => void;
}) {
  const id = useId();
  const actions = useActions();
  const busy = actions.isBusy("antigravity-proxy-timezone");
  const applyProxyTimezone = () =>
    void actions.run("antigravity-proxy-timezone", async () => {
      const result = await request<{ timezone: string }>(
        `/proxies/${encodeURIComponent(value.proxy_id ?? "")}/check/timezone`,
        { method: "POST", body: {} },
      );
      onChange({ ...value, timezone: result.timezone });
    });
  return { id, busy, applyProxyTimezone } as const;
}

type OfficialData = {
  value: Json;
  observed_at?: string;
  refresh_error?: string;
  groups?: SupplierQuotaGroup[] | null;
};

export const antigravityQuotaGroups = (groups: SupplierQuotaGroup[]) =>
  groups.map((group) => ({
    name: group.name,
    windows: group.windows.map((window) => ({
      id: window.id,
      name: window.name || window.window,
      remainingPercent:
        window.remaining_fraction == null ? null : Math.round(window.remaining_fraction * 100),
      resetAt: window.reset_at,
      disabled: window.disabled,
    })),
  }));

/** Official details and grouped quota windows, read from persisted snapshots. */
export function useAntigravityOfficialData({
  id,
  section,
  onUpdated,
}: {
  id: string;
  section: string;
  onUsername?: (username: string | undefined) => void;
  onUpdated?: () => void;
}) {
  const actions = useActions();
  const now = useQuotaClock();
  const path = `/suppliers/${id}/official?section=${section}`;
  const resource = useResource<OfficialData>(path);
  const refreshKey = `antigravity-official:${id}:${section}`;
  const refreshing = actions.isBusy(refreshKey);
  const refreshOfficial = () =>
    void actions.run(refreshKey, async () => {
      const result = await request<OfficialData>(`${path}&refresh=true`);
      if (result.refresh_error) throw new Error(result.refresh_error);
      resource.reload();
    });
  useErrorToast(resource.error);
  useErrorToast(resource.data?.refresh_error);
  useEffect(() => {
    if (resource.data) onUpdated?.();
  }, [resource.data, onUpdated]);
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
  const groups = resource.data?.groups ?? [];
  return { refreshOfficial, refreshing, now, resource, fields, groups } as const;
}
