"use client";
import { useErrorToast } from "@/lib/actions";
import { type Subscription } from "@/lib/api";
import { useListResource } from "@/lib/pagination";

export function useSubscriptionsPage() {
  const subscriptions = useListResource<Subscription>("/subscriptions");
  const subscriptionPage = subscriptions.pagination;
  useErrorToast(subscriptions.error);

  return { subscriptions, subscriptionPage } as const;
}
