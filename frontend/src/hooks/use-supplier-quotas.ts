"use client";
import { useEffect, useState } from "react";
import { request, type Supplier } from "@/lib/api";
import { toastError } from "@/lib/actions";

/** Cached reads only; filter and view changes do not trigger upstream refreshes. */
export function useSupplierQuotas(accounts: Supplier[] | undefined, resetIds?: readonly string[]) {
  const [updates, setUpdates] = useState<{ source: Supplier[]; items: Record<string, Supplier> }>();
  useEffect(() => {
    if (!accounts) return;
    const controller = new AbortController();
    async function refreshMissing() {
      for (const account of accounts!) {
        if (controller.signal.aborted) break;
        if (
          resetIds?.includes(account.id) ||
          !["active", "quota_exhausted"].includes(account.status) ||
          !account.authorized ||
          (account.quota && !account.quota.stale)
        )
          continue;
        try {
          const item = await request<Supplier>(`/suppliers/${account.id}/quota`, {
            signal: controller.signal,
          });
          if (!controller.signal.aborted)
            setUpdates((current) => ({
              source: accounts!,
              items: {
                ...(current && current.source === accounts ? current.items : {}),
                [item.id]: item,
              },
            }));
        } catch (error) {
          if (controller.signal.aborted) break;
          toastError(error);
          // Read the persisted failure and keep the last successful quota snapshot.
          try {
            const item = await request<Supplier>(`/suppliers/${account.id}`, {
              signal: controller.signal,
            });
            if (!controller.signal.aborted)
              setUpdates((current) => ({
                source: accounts!,
                items: {
                  ...(current && current.source === accounts ? current.items : {}),
                  [item.id]: item,
                },
              }));
          } catch {
            /* The original failure has already been reported. */
          }
        }
      }
    }
    void refreshMissing();
    return () => controller.abort();
  }, [accounts, resetIds]);
  return (
    accounts?.map((account) =>
      updates?.source === accounts ? (updates.items[account.id] ?? account) : account,
    ) ?? []
  );
}

/** An explicit list refresh checks official quotas, including still-fresh cache. */
export async function refreshSupplierQuotas(accounts: Supplier[]) {
  const pending = accounts.filter(
    (account) =>
      account.authorized &&
      !account.authentication_invalid &&
      ["active", "quota_exhausted"].includes(account.status),
  );
  const failures: string[] = [];
  await Promise.all(
    Array.from({ length: Math.min(4, pending.length) }, async () => {
      for (let account = pending.shift(); account; account = pending.shift()) {
        try {
          await request<Supplier>(`/suppliers/${account.id}/quota?refresh=true`);
        } catch {
          failures.push(account.email || account.display_name || account.id);
        }
      }
    }),
  );
  return failures;
}

/** A local clock tick never performs network I/O. */
export function useQuotaClock() {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30000);
    return () => window.clearInterval(timer);
  }, []);
  return now;
}
