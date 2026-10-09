"use client";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
import { Progress } from "@/components/ui/progress";
import type { Json } from "@/lib/api";
import { date } from "@/lib/format";
import { useGrokOfficialData } from "@/lib/providers/grok/data";
import { subscriptionLabel } from "@/lib/providers/grok/subscriptions";
import { percentLabel, quotaResetLabel, quotaWindowLabel } from "@/lib/supplier-state";

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
  const { setRefresh, now, resource, fields } = useGrokOfficialData({ id, section });
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
