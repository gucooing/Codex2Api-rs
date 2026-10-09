"use client";
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
import { type Device } from "@/lib/api";
import { scopes } from "@/lib/domain";
import { date } from "@/lib/format";
import { ChevronRight } from "lucide-react";

type Props = {
  device: Device;
};

export function DeviceDialog({ device }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
          aria-label={"查看详情：" + String(device.user_agent || "未知客户端")}
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">{device.user_agent || "未知客户端"}</span>
            <span className="block truncate text-xs text-muted-foreground">
              {device.installation_id ?? "未提供安装标识"}
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
            <FieldTitle>客户端 / 安装标识</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {device.user_agent || "未知客户端"}
              <CardDescription className="break-all font-mono text-xs">
                {device.installation_id ?? "未提供安装标识"}
              </CardDescription>
            </div>
          </Field>
          <Field>
            <FieldTitle>授权范围</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {device.scopes
                .split(/\s+/)
                .map((scope) => scopes[scope] ?? scope)
                .join("、")}
            </div>
          </Field>
          <Field>
            <FieldTitle>首次登录</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {date(device.authenticated_at_ms ?? device.created_at)}
            </div>
          </Field>
          <Field>
            <FieldTitle>最近续期 / 使用</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {date(device.last_login_at)}
              <CardDescription>{date(device.last_used_at)}</CardDescription>
            </div>
          </Field>
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
