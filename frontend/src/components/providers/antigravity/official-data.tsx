"use client";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
import { date } from "@/lib/format";
import { useAntigravityOfficialData } from "@/lib/providers/antigravity/data";
import { AntigravityQuotaGroups } from "./quota";

export function AntigravityOfficialData({
  id,
  section,
  onUpdated,
}: {
  id: string;
  section: string;
  onUsername?: (username: string | undefined) => void;
  onUpdated?: () => void;
}) {
  const { refreshOfficial, refreshing, now, resource, fields, groups } = useAntigravityOfficialData(
    {
      id,
      section,
      onUpdated,
    },
  );
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {section === "quota" ? "Antigravity 模型额度" : "Antigravity 官方资料"}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" disabled={refreshing} onClick={refreshOfficial}>
            从官方刷新
          </Button>
          <CardDescription>采集时间：{date(resource.data?.observed_at)}</CardDescription>
        </div>
        {section === "quota" ? (
          <div className="max-w-md space-y-3">
            <AntigravityQuotaGroups groups={groups} now={now} />
            {!groups.length && (
              <p className="text-sm text-muted-foreground">
                {resource.loading ? "加载中…" : "暂无官方额度快照"}
              </p>
            )}
          </div>
        ) : (
          <FieldGroup className="grid gap-3 sm:grid-cols-2">
            {fields.map(([label, value]) => (
              <Field key={label}>
                <FieldTitle>{label}</FieldTitle>
                <FieldDescription>{value || "—"}</FieldDescription>
              </Field>
            ))}
          </FieldGroup>
        )}
      </CardContent>
    </Card>
  );
}
