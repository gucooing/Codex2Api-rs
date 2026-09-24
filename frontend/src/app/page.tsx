"use client";
import Link from "next/link";
import { useResource } from "@/lib/hooks";
import { useErrorToast } from "@/lib/actions";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";

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
      {error && (
        <Button variant="outline" size="sm" onClick={reload}>
          重新加载
        </Button>
      )}
      {
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          {[
            { label: "供应账户", value: data?.supplier_count ?? "—", href: "/suppliers/" },
            { label: "虚拟账户", value: data?.consumer_count ?? "—", href: "/consumers/" },
            { label: "已启用虚拟账户", value: data?.enabled_consumers ?? "—", href: "/consumers/" },
            { label: "模型配置", value: data?.models_count ?? "—", href: "/models/" },
          ].map((item) => (
            <Card key={item.label}>
              <CardHeader>
                <CardDescription>{item.label}</CardDescription>
                <CardTitle className="text-2xl tabular-nums">{item.value}</CardTitle>
              </CardHeader>
              <CardContent>
                <Button asChild variant="outline" size="sm">
                  <Link href={item.href}>查看</Link>
                </Button>
              </CardContent>
            </Card>
          ))}
        </div>
      }
    </>
  );
}
