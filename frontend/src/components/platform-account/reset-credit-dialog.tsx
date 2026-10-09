"use client";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldTitle } from "@/components/ui/field";
import { date } from "@/lib/format";
import type { ResetCreditRecord } from "@/lib/platform-account";
import { ChevronRight } from "lucide-react";

type Props = {
  credit: ResetCreditRecord;
};

export function ResetCreditDialog({ credit }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
          aria-label={
            "查看详情：" + String(credit.source === "admin_reset" ? "管理员直接重置" : "重置卡")
          }
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">
              {credit.source === "admin_reset" ? "管理员直接重置" : "重置卡"}
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
            <FieldTitle>类型</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {credit.source === "admin_reset" ? "管理员直接重置" : "重置卡"}
            </div>
          </Field>
          <Field>
            <FieldTitle>发放时间</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">{date(credit.granted_at)}</div>
          </Field>
          <Field>
            <FieldTitle>启用时间</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">{date(credit.available_at)}</div>
          </Field>
          <Field>
            <FieldTitle>到期时间</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">{date(credit.expires_at)}</div>
          </Field>
          <Field>
            <FieldTitle>状态</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <Badge variant="outline">
                {
                  {
                    available: "可用",
                    redeemed: "已使用",
                    pending: "待启用",
                    expired: "已过期",
                    not_applied: "未执行",
                  }[credit.status]
                }
              </Badge>
            </div>
          </Field>
          <Field>
            <FieldTitle>使用时间</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">{date(credit.redeemed_at)}</div>
          </Field>
          <Field>
            <FieldTitle>使用方</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {credit.redeemed_by === "admin"
                ? "管理员"
                : credit.redeemed_by === "client"
                  ? "客户端"
                  : "—"}
            </div>
          </Field>
          <Field>
            <FieldTitle>重置窗口数</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {credit.status === "redeemed" ? credit.windows_reset : "—"}
            </div>
          </Field>
          <Field>
            <FieldTitle>管理备注</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">{credit.note || "—"}</div>
          </Field>
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
