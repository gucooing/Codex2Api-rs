"use client";
import { SubscriptionRoute } from "@/app/components/subscriptions/subscriptions";

import { Suspense } from "react";

export default function Page() {
  return (
    <Suspense>
      <SubscriptionRoute />
    </Suspense>
  );
}
