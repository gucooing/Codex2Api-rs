"use client";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import { type Supplier } from "@/lib/api";
import {
  cycleUsageLabel,
  cycleUsageTitle,
  quotaResetLabel,
  quotaWindowLabel,
} from "@/lib/supplier-state";
import { tokenCount } from "@/lib/usage-display";

export function LocalUsage({ account }: { account?: Supplier }) {
  const now = useQuotaClock();
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          本地用量
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <FieldGroup className="grid gap-3 sm:grid-cols-2" aria-label="周期使用额度">
          {account?.quota?.windows.map((window) => (
            <Field key={window.id}>
              <FieldTitle>{quotaWindowLabel(window)}周期已用</FieldTitle>
              <FieldDescription className="tabular-nums" title={cycleUsageTitle(window)}>
                {cycleUsageLabel(window)}
              </FieldDescription>
              <FieldDescription>{quotaResetLabel(window.reset_at, now)}</FieldDescription>
            </Field>
          ))}
          {!account?.quota?.windows.length && (
            <FieldDescription>暂无官方额度周期，暂不能统计周期用量。</FieldDescription>
          )}
        </FieldGroup>
        <FieldGroup className="grid gap-3 sm:grid-cols-2">
          <Field>
            <FieldTitle>累计 Token</FieldTitle>
            <FieldDescription>{tokenCount(account?.usage?.lifetime_tokens)}</FieldDescription>
          </Field>
          <Field>
            <FieldTitle>单日峰值 Token</FieldTitle>
            <FieldDescription>{tokenCount(account?.usage?.peak_daily_tokens)}</FieldDescription>
          </Field>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
