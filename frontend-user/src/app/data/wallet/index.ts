"use client";
import { useErrorToast } from "@/lib/actions";
import { type Entry } from "@/lib/api";
import { useListResource } from "@/lib/pagination";

export function useWalletPage() {
  const wallet = useListResource<Entry, { balance_usd: string }>("/wallet");
  const walletPage = wallet.pagination;
  useErrorToast(wallet.error);

  return { wallet, walletPage } as const;
}
