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
    normal_consumer_count: number;
    user_count: number;
    active_user_count: number;
    models_count: number;
  }>(`/overview?tz_offset=${new Date().getTimezoneOffset()}`);
  useErrorToast(error);
  return (
    <>
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-4" aria-label="账户与模型概况">
        {[
          { label: "供应账户", value: data?.supplier_count ?? "—", href: "/suppliers/" },
          {
            label: "虚拟账户",
            value: data ? `${data.normal_consumer_count}/${data.consumer_count}` : "—",
            href: "/consumers/",
            hint: "正常 / 总数",
          },
          {
            label: "用户数",
            value: data ? `${data.active_user_count}/${data.user_count}` : "—",
            href: "/users/",
            hint: "当日活跃 / 总数",
          },
          { label: "计费配置", value: data?.models_count ?? "—", href: "/models/" },
        ].map((item) => (
          <Button
            key={item.label}
            asChild
            variant="outline"
            className="h-12 justify-between gap-2 px-3"
          >
            <Link href={item.href} title={item.hint}>
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
