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
import { type Proxy } from "@/lib/api";
import { date } from "@/lib/format";
import { ChevronRight } from "lucide-react";

type Props = {
  proxy: Proxy;
};

export function ProxyRecordDialog({ proxy }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
          aria-label={"查看详情：" + String(proxy.name)}
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">{proxy.name}</span>
            <span className="block truncate text-xs text-muted-foreground">
              {proxy.host + ":" + proxy.port}
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
            <FieldTitle>代理</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <strong>{proxy.name}</strong>
              <CardDescription>{proxy.display_url}</CardDescription>
            </div>
          </Field>
          <Field>
            <FieldTitle>绑定账户</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">{proxy.account_count}</div>
          </Field>
          <Field>
            <FieldTitle>出口 / 时区</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {[proxy.exit_ip, proxy.country, proxy.region, proxy.city]
                .filter(Boolean)
                .join(" · ") || "未检查"}
              <CardDescription>{proxy.timezone ?? "时区未获取"}</CardDescription>
            </div>
          </Field>
          <Field>
            <FieldTitle>连接检查</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {proxy.connection_ok === null ? (
                <span className="text-sm text-muted-foreground">未检查</span>
              ) : (
                <Badge variant={proxy.connection_ok ? "secondary" : "outline"}>
                  {proxy.connection_ok ? "检查成功" : "连接失败"}
                </Badge>
              )}
              <CardDescription>
                {proxy.connection_latency_ms === null ? "" : `${proxy.connection_latency_ms} ms`}
              </CardDescription>
              <CardDescription className="max-w-80 whitespace-normal break-words">
                {proxy.connection_error}
              </CardDescription>
              <CardDescription>{date(proxy.connection_checked_at)}</CardDescription>
            </div>
          </Field>
          <Field>
            <FieldTitle>质量检查</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {proxy.quality_ok === null ? (
                <span className="text-sm text-muted-foreground">未检查</span>
              ) : (
                <Badge variant={proxy.quality_ok ? "secondary" : "outline"}>
                  {proxy.quality_ok ? "检查通过" : "检查失败"}
                </Badge>
              )}
              <CardDescription>
                {proxy.quality_latency_ms === null ? "" : `${proxy.quality_latency_ms} ms`}
              </CardDescription>
              <CardDescription className="max-w-80 whitespace-normal break-words">
                {proxy.quality_error}
              </CardDescription>
              <CardDescription>{date(proxy.quality_checked_at)}</CardDescription>
            </div>
          </Field>
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
