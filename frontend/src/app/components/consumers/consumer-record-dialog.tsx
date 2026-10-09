"use client";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardDescription } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldTitle } from "@/components/ui/field";
import { Progress } from "@/components/ui/progress";
import type { SupplierQuotaWindow } from "@/lib/api";
import { type Consumer } from "@/lib/api";
import { date } from "@/lib/format";
import { subscriptionLabel } from "@/lib/platform-account";
import { percentLabel, quotaResetLabel, quotaWindowLabel } from "@/lib/supplier-state";
import { ChevronRight } from "lucide-react";
import Link from "next/link";

type Props = {
  account: Consumer & { quota: { windows: SupplierQuotaWindow[] } };
  now: number;
};

export function ConsumerRecordDialog({ account, now }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
          aria-label={"查看详情：" + String(account.name)}
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">{account.name}</span>
            <span className="block truncate text-xs text-muted-foreground">{account.username}</span>
          </span>
          <ChevronRight className="size-3 shrink-0" />
        </Button>
      </DialogTrigger>
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>记录详情</DialogTitle>
          <DialogDescription>当前记录的完整字段</DialogDescription>
        </DialogHeader>
        <FieldGroup className="gap-3">
          <Field>
            <FieldTitle>虚拟账户</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <Link
                className="block max-w-56 truncate"
                title={account.name}
                href={`/consumers/detail/?id=${encodeURIComponent(account.id)}`}
              >
                <strong>{account.name}</strong>
              </Link>
              <CardDescription
                className="max-w-56 truncate"
                title={`${account.username} · ${account.email}`}
              >
                {account.username} · {account.email}
              </CardDescription>
            </div>
          </Field>
          <Field>
            <FieldTitle>提供商</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">{account.provider_id}</div>
          </Field>
          <Field>
            <FieldTitle>当前权益</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {account.plan_name}
              <CardDescription>{subscriptionLabel(account.subscription_status)}</CardDescription>
            </div>
          </Field>
          <Field>
            <FieldTitle>订阅到期</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {account.subscription_expires_at
                ? date(account.subscription_expires_at)
                : "未设置到期时间"}
            </div>
          </Field>
          <Field>
            <FieldTitle>登录状态</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <Badge variant={account.enabled ? "secondary" : "outline"}>
                {account.enabled ? "已启用" : "已停用"}
              </Badge>
            </div>
          </Field>
          <Field>
            <FieldTitle>额度</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <div className="flex w-64 flex-wrap gap-2" aria-label="账户额度">
                {account.quota.windows.map((window) => (
                  <div key={window.id} className="min-w-0 flex-1 basis-28 space-y-1 text-xs">
                    <div
                      className="truncate"
                      title={
                        window.reset_at == null
                          ? "首次使用后计时"
                          : quotaResetLabel(window.reset_at, now)
                      }
                    >
                      {quotaWindowLabel(window)}：
                      {window.reset_at == null
                        ? "首次使用后计时"
                        : quotaResetLabel(window.reset_at, now)}
                    </div>
                    <div className="flex items-center gap-2">
                      {window.used_percent != null ? (
                        <>
                          <Progress
                            value={Math.min(100, window.used_percent)}
                            className="h-1 flex-1 [&>[data-slot=progress-indicator]]:bg-foreground"
                            aria-label={quotaWindowLabel(window) + "额度已用"}
                          />
                          <span className="shrink-0 whitespace-nowrap tabular-nums">
                            {percentLabel(window.used_percent)}
                          </span>
                        </>
                      ) : (
                        <span className="text-muted-foreground">不限额</span>
                      )}
                    </div>
                  </div>
                ))}
                {account.quota.windows.length === 0 && (
                  <span className="text-xs text-muted-foreground">不限额</span>
                )}
              </div>
            </div>
          </Field>
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
