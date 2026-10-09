"use client";
import { SubscriptionDetail } from "@/app/components/subscriptions/subscription-detail";
import { Spinner } from "@/components/ui/spinner";
import { PlatformApiContext } from "@/lib/platform-scope";
import { Suspense } from "react";
export default function Page() {
  return (
    <PlatformApiContext value="/subscriptions">
      <Suspense fallback={<Spinner className="my-6 size-5" role="status" aria-label="正在加载" />}>
        <SubscriptionDetail />
      </Suspense>
    </PlatformApiContext>
  );
}
