"use client";
import { ConsumerDetail } from "@/app/components/consumers/consumer-detail";
import { Spinner } from "@/components/ui/spinner";
import { PlatformApiContext } from "@/lib/platform-scope";
import { Suspense } from "react";
export default function Page() {
  return (
    <PlatformApiContext value="/consumers">
      <Suspense fallback={<Spinner className="my-6 size-5" role="status" aria-label="正在加载" />}>
        <ConsumerDetail />
      </Suspense>
    </PlatformApiContext>
  );
}
