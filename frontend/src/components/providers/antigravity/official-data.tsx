"use client";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
import { Progress } from "@/components/ui/progress";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { date } from "@/lib/format";
import { useAntigravityOfficialData } from "@/lib/providers/antigravity/data";
import { percentLabel, quotaResetLabel } from "@/lib/supplier-state";

export function AntigravityOfficialData({
  id,
  section,
}: {
  id: string;
  section: string;
  onUsername?: (username: string | undefined) => void;
  onUpdated?: () => void;
}) {
  const { setRefresh, now, resource, fields, models } = useAntigravityOfficialData({
    id,
    section,
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {section === "quota" ? "Antigravity 模型额度" : "Antigravity 官方资料"}
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
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>模型</TableHead>
                <TableHead>已用</TableHead>
                <TableHead>重置</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {models.map((item) => (
                <TableRow key={item.key}>
                  <TableCell>
                    <div className="font-medium">{item.name}</div>
                    <div className="text-xs text-muted-foreground">{item.model}</div>
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <Progress value={item.usedPercent ?? undefined} className="w-32" />
                      <span className="text-sm">
                        {item.usedPercent == null ? "未知" : percentLabel(item.usedPercent)}
                      </span>
                    </div>
                  </TableCell>
                  <TableCell>
                    {item.resetAt == null ? "—" : quotaResetLabel(item.resetAt, now)}
                  </TableCell>
                </TableRow>
              ))}
              {!models.length && (
                <TableRow>
                  <TableCell colSpan={3} className="text-center text-muted-foreground">
                    {resource.loading ? "加载中…" : "官方未提供模型额度"}
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
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
