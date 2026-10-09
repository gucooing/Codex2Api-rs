"use client";
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
import { type Json } from "@/lib/api";
import { subscriptionLabel } from "@/lib/subscriptions";
import { ChevronRight } from "lucide-react";

type Props = {
  account: { [key: string]: Json };
};

export function AccountRecordDialog({ account }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
          aria-label={
            "查看详情：" + String(String(account.name ?? account.account_id ?? account.id ?? "—"))
          }
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">
              {String(account.name ?? account.account_id ?? account.id ?? "—")}
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
            <FieldTitle>账户</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {String(account.name ?? account.account_id ?? account.id ?? "—")}
            </div>
          </Field>
          <Field>
            <FieldTitle>类型</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {String(account.structure ?? "—")}
            </div>
          </Field>
          <Field>
            <FieldTitle>订阅</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {subscriptionLabel(account.plan_type)}
            </div>
          </Field>
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
