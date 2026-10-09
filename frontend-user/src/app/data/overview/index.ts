"use client";
import { useErrorToast } from "@/lib/actions";
import { money, type Subscription } from "@/lib/api";
import { type OrderPage } from "@/lib/orders";
import { useResource } from "@/lib/resource";
import { tokens, usageCost, usageSeries, type UserUsage } from "@/lib/usage";
import { useState } from "react";

export function useOverviewPage() {
  const [days, setDays] = useState("7");
  const subscriptions = useResource<{ items: Subscription[] }>("/subscriptions", 0);
  const wallet = useResource<{ balance_usd: string }>("/wallet", 0);
  const orders = useResource<OrderPage>("/orders?limit=5", 0);
  const usage = useResource<UserUsage>(
    `/usage?days=${days}&tz_offset=${new Date().getTimezoneOffset()}&limit=10`,
    0,
  );
  useErrorToast(subscriptions.error);
  useErrorToast(wallet.error);
  useErrorToast(orders.error);
  useErrorToast(usage.error);
  const refresh = () => {
    subscriptions.reload();
    wallet.reload();
    orders.reload();
    usage.reload();
  };
  const summary = usage.data?.summary;
  const series = usageSeries(usage.data);
  const cards = [
    {
      label: "钱包余额（USD）",
      value: wallet.data ? money(wallet.data.balance_usd) : "—",
      href: "/wallet/",
    },
    { label: "请求次数", value: summary?.request_count?.toLocaleString() ?? "—", href: "/usage/" },
    { label: "Token 用量", value: tokens(summary?.total_tokens), href: "/usage/" },
    { label: "已计费用量（USD）", value: usageCost(summary?.cost_nano_usd), href: "/usage/" },
  ];

  return { days, setDays, subscriptions, orders, refresh, summary, series, cards } as const;
}
