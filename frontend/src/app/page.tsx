"use client";
import Link from "next/link";
import { useResource } from "@/lib/hooks";
import { useErrorToast } from "@/lib/actions";
import { Button } from "@/components/ui/button";
import { OverviewStatistics } from "@/components/overview-statistics";

export default function Overview() {
  const { data, error, reload } = useResource<{
    supplier_count: number;
    consumer_count: number;
    enabled_consumers: number;
    models_count: number;
  }>("/overview");
  useErrorToast(error);
  return (
    <>
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-4" aria-label="账户与模型概况">
        {[
          { label: "供应账户", value: data?.supplier_count ?? "—", href: "/suppliers/" },
          { label: "虚拟账户", value: data?.consumer_count ?? "—", href: "/consumers/" },
          { label: "已启用虚拟账户", value: data?.enabled_consumers ?? "—", href: "/consumers/" },
          { label: "模型配置", value: data?.models_count ?? "—", href: "/models/" },
        ].map((item) => (
          <Button
            key={item.label}
            asChild
            variant="outline"
            className="h-12 justify-between gap-2 px-3"
          >
            <Link href={item.href}>
              <span className="text-xs text-muted-foreground">{item.label}</span>
              <span className="text-lg font-semibold tabular-nums">{item.value}</span>
            </Link>
          </Button>
        ))}
      </div>
      <OverviewStatistics onRefresh={reload} />
    </>
  );
}
