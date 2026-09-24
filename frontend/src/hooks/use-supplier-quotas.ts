"use client";
import { useEffect, useState } from "react";
import { request, type Supplier } from "@/lib/api";
import { toastError } from "@/lib/actions";

/** Cached reads only; filter and view changes do not trigger upstream refreshes. */
export function useSupplierQuotas(accounts: Supplier[] | undefined) {
  const [updates, setUpdates] = useState<{ source: Supplier[]; items: Record<string, Supplier> }>();
  useEffect(() => {
    if (!accounts) return;
    const controller = new AbortController();
    async function refreshMissing() {
      for (const account of accounts!) {
        if (controller.signal.aborted) break;
        if (
          account.status !== "active" ||
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
  }, [accounts]);
  return (
    accounts?.map((account) =>
      updates?.source === accounts ? (updates.items[account.id] ?? account) : account,
    ) ?? []
  );
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
