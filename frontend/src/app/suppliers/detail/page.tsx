import { SupplierDetail } from "@/app/components/suppliers/supplier-detail";
import { Spinner } from "@/components/ui/spinner";
import { Suspense } from "react";

export default function Page() {
  return (
    <Suspense fallback={<Spinner className="my-6 size-5" role="status" aria-label="正在加载" />}>
      <SupplierDetail />
    </Suspense>
  );
}
