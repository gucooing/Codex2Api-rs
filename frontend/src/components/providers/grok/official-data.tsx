"use client";
import { useState } from "react";
import { useResource } from "@/lib/hooks";
import { useErrorToast } from "@/lib/actions";
import { date } from "@/lib/format";
import type { Json, SupplierQuota } from "@/lib/api";
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Field, FieldTitle, FieldDescription, FieldGroup } from "@/components/ui/field";
import { Progress } from "@/components/ui/progress";
import { quotaWindowLabel, percentLabel, quotaResetLabel } from "@/lib/supplier-state";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import { subscriptionLabel } from "@/lib/providers/grok/subscriptions";

function at(root: Json | undefined, path: string[]): Json | undefined {
  return path.reduce<Json | undefined>(
    (v, key) => (v && typeof v === "object" && !Array.isArray(v) ? v[key] : undefined),
    root,
  );
}
export function GrokOfficialData({
  id,
  section,
}: {
  id: string;
  section: string;
  onUsername?: (username: string | undefined) => void;
  onUpdated?: () => void;
}) {
  const [refresh, setRefresh] = useState(0);
  const now = useQuotaClock();
  const resource = useResource<{
    value: Json;
    quota?: SupplierQuota;
    observed_at?: string;
    refresh_error?: string;
  }>(`/suppliers/${id}/official?section=${section}${refresh ? `&refresh=true&r=${refresh}` : ""}`);
  useErrorToast(resource.error);
  useErrorToast(resource.data?.refresh_error);
  const fields =
    section === "details"
      ? [
          ["用户编号", ["userId"]],
          ["邮箱", ["email"]],
          ["名字", ["firstName"]],
          ["姓氏", ["lastName"]],
          ["身份类型", ["principalType"]],
          ["团队编号", ["teamId"]],
          ["团队名称", ["teamName"]],
          ["订阅", ["subscriptionTierDisplay"]],
        ]
      : section === "usage"
        ? [
            ["订阅", ["subscription_tier_display"]],
            ["Grok Build 权限", ["allow_access"]],
            ["按需使用", ["on_demand_enabled"]],
            ["服务提示", ["gate_message"]],
          ]
        : [
            ["已用比例", ["config", "creditUsagePercent"]],
            ["周期开始", ["config", "currentPeriod", "start"]],
            ["周期结束", ["config", "currentPeriod", "end"]],
            ["预付余额（美分）", ["config", "prepaidBalance", "val"]],
            ["按需已用（美分）", ["config", "onDemandUsed", "val"]],
          ];
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {
            (
              {
                details: "Grok 官方资料",
                usage: "Grok 服务配置",
                quota: "Grok 官方额度",
                credits: "Grok 官方余额",
              } as Record<string, string>
            )[section]
          }
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" onClick={() => setRefresh(Date.now())}>
            从官方刷新
          </Button>
          <CardDescription>采集时间：{date(resource.data?.observed_at)}</CardDescription>
        </div>
        {section === "quota" ? (
          <div className="grid gap-3 sm:grid-cols-2">
            {resource.data?.quota?.windows?.map((w) => (
              <div key={w.id} className="space-y-1">
                <CardDescription>
                  {quotaWindowLabel(w)}：{quotaResetLabel(w.reset_at, now)}
                </CardDescription>
                <div className="flex items-center gap-2">
                  <Progress value={w.used_percent ?? undefined} />
                  <span className="text-sm">
                    {w.used_percent == null ? "未知" : percentLabel(w.used_percent)}
                  </span>
                </div>
              </div>
            ))}
            {!resource.data?.quota?.windows?.length && (
              <CardDescription>官方未提供额度窗口</CardDescription>
            )}
          </div>
        ) : (
          <FieldGroup className="grid gap-3 sm:grid-cols-2">
            {fields.map(([label, path]) => {
              const value =
                at(resource.data?.value, path as string[]) ??
                (section === "details" && label === "订阅"
                  ? subscriptionLabel(at(resource.data?.value, ["subscriptionTier"]))
                  : undefined);
              return (
                <Field key={String(label)}>
                  <FieldTitle>{String(label)}</FieldTitle>
                  <FieldDescription>
                    {typeof value === "boolean"
                      ? value
                        ? "启用"
                        : "关闭"
                      : typeof value === "string" || typeof value === "number"
                        ? String(value)
                        : "—"}
                  </FieldDescription>
                </Field>
              );
            })}
          </FieldGroup>
        )}
      </CardContent>
    </Card>
  );
}
