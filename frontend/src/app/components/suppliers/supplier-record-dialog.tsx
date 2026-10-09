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
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
import { Progress } from "@/components/ui/progress";
import { type Supplier } from "@/lib/api";
import { date } from "@/lib/format";
import { supplierSubscriptionLabel as subscriptionLabel } from "@/lib/subscriptions";
import {
  cycleUsageLabel,
  cycleUsageTitle,
  percentLabel,
  quotaResetLabel,
  quotaWindowLabel,
  supplierStatusLabel,
} from "@/lib/supplier-state";
import { ChevronRight } from "lucide-react";
import Link from "next/link";

type Props = {
  item: Supplier;
  now: number;
};

export function SupplierRecordDialog({ item, now }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
          aria-label={"查看详情：" + String(item.email || item.display_name || item.id)}
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">
              {item.email || item.display_name || item.id}
            </span>
            <span className="block truncate text-xs text-muted-foreground">
              {subscriptionLabel(item.plan_type, item.provider_id)}
            </span>
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
            <FieldTitle>供应账户</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <Link href={`/suppliers/detail/?id=${encodeURIComponent(item.id)}`}>
                <strong>{item.email || item.display_name || item.id}</strong>
              </Link>
              <CardDescription>{item.email}</CardDescription>
            </div>
          </Field>
          <Field>
            <FieldTitle>提供商 / 订阅</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {item.provider_id}
              <CardDescription>
                {subscriptionLabel(item.plan_type, item.provider_id)}
              </CardDescription>
            </div>
          </Field>
          <Field>
            <FieldTitle>套餐到期</FieldTitle>
            <FieldDescription>{date(item.subscription_expires_at)}</FieldDescription>
          </Field>
          <Field>
            <FieldTitle>状态</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <Badge
                title={item.error_message ?? undefined}
                variant={
                  item.status === "error"
                    ? "destructive"
                    : item.status === "active"
                      ? "secondary"
                      : "outline"
                }
              >
                {supplierStatusLabel(item.status)}
              </Badge>
            </div>
          </Field>
          <Field>
            <FieldTitle>额度</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <div
                className="flex w-80 flex-wrap gap-2"
                aria-label="官方额度"
                title={item.quota ? "缓存于 " + date(item.quota.observed_at) : "暂无额度缓存"}
              >
                {item.quota?.windows?.map((window) => (
                  <div key={window.id} className="min-w-0 flex-1 basis-64 space-y-1 text-xs">
                    <div className="flex items-center justify-between gap-2">
                      <span className="truncate" title={quotaResetLabel(window.reset_at, now)}>
                        {quotaWindowLabel(window)}：{quotaResetLabel(window.reset_at, now)}
                      </span>
                      <span className="shrink-0 tabular-nums" title={cycleUsageTitle(window)}>
                        {cycleUsageLabel(window)}
                      </span>
                    </div>
                    <div className="flex items-center gap-2">
                      {window?.used_percent != null ? (
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
                        <span className="text-muted-foreground">额度未提供</span>
                      )}
                    </div>
                  </div>
                ))}
                {!item.quota?.windows?.length && (
                  <span className="text-xs text-muted-foreground">
                    {item.quota?.windows
                      ? "官方未提供额度窗口"
                      : item.quota
                        ? "额度数据未加载"
                        : "暂无额度缓存"}
                  </span>
                )}
              </div>
            </div>
          </Field>
          <Field>
            <FieldTitle>标签</FieldTitle>
            <div className="flex flex-wrap items-center gap-1">
              {(item.tag_ids ?? []).map((id) => (
                <Badge variant="secondary" key={id}>
                  {item.tags?.find((tag) => tag.id === id)?.name ?? "标签"}
                </Badge>
              ))}
              {!item.tag_ids?.length && <span className="text-muted-foreground">—</span>}
            </div>
          </Field>
          <Field>
            <FieldTitle>绑定数</FieldTitle>
            <FieldDescription>{item.binding_count ?? 0}</FieldDescription>
          </Field>
          <Field>
            <FieldTitle>最近使用</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">{date(item.last_used_at)}</div>
          </Field>
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
