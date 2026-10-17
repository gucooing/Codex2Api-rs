"use client";
import { OverviewStatistics } from "@/app/components/overview/overview-statistics";
import { useOverview } from "@/app/data/overview";
import { Button } from "@/components/ui/button";
import Link from "next/link";

export default function Overview() {
  const { data, reload } = useOverview();
  return (
    <>
      <div className="grid grid-cols-2 gap-2.5 lg:grid-cols-4" aria-label="账户与模型概况">
        {[
          {
            label: "供应账户",
            value: data?.supplier_count ?? "—",
            href: "/suppliers/",
          },
          {
            label: "订阅",
            value: data ? `${data.normal_consumer_count}/${data.consumer_count}` : "—",
            href: "/subscriptions/",
            hint: "正常 / 总数",
          },
          {
            label: "用户数",
            value: data ? `${data.active_user_count}/${data.user_count}` : "—",
            href: "/users/",
            hint: "当日活跃 / 总数",
          },
          {
            label: "计费配置",
            value: data?.models_count ?? "—",
            href: "/models/",
          },
        ].map((item) => (
          <Button
            key={item.label}
            asChild
            variant="outline"
            className="h-12 justify-between gap-2 rounded-xl bg-card px-3.5 shadow-xs hover:bg-accent/50"
          >
            <Link href={item.href} title={item.hint}>
              <span className="truncate text-xs font-medium text-muted-foreground">
                {item.label}
              </span>
              <span className="shrink-0 text-lg font-semibold tracking-tight tabular-nums">
                {item.value}
              </span>
            </Link>
          </Button>
        ))}
      </div>
      <OverviewStatistics onRefresh={reload} />
    </>
  );
}
