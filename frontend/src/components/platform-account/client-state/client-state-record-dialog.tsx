"use client";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
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
import { clientScalar } from "@/lib/platform-account/client-state";
import { obj } from "@/lib/platform-account/config";
import { scalar } from "@/lib/records";
import { ChevronDown, ChevronRight } from "lucide-react";
import { NamedClientState } from "./named-client-state";

type Props = {
  raw: Json;
  row: { [key: string]: Json };
  path: string;
  section: string;
  pagination: { page: number; size: { value: string } };
  index: number;
};

export function ClientStateRecordDialog({ raw, row, path, section, pagination, index }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
          aria-label={
            "查看详情：" +
            String(
              typeof raw === "string"
                ? raw
                : scalar(row.title ?? row.name ?? row.email ?? row.category ?? obj(row.gizmo).name),
            )
          }
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">
              {typeof raw === "string"
                ? raw
                : scalar(row.title ?? row.name ?? row.email ?? row.category ?? obj(row.gizmo).name)}
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
            <FieldTitle>名称 / 内容</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {typeof raw === "string"
                ? raw
                : scalar(row.title ?? row.name ?? row.email ?? row.category ?? obj(row.gizmo).name)}
            </div>
          </Field>
          <Field>
            <FieldTitle>标识</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {scalar(row.id ?? row.item_id ?? obj(row.gizmo).id)}
            </div>
          </Field>
          <Field>
            <FieldTitle>状态</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {clientScalar(row.status ?? row.enabled)}
            </div>
          </Field>
          <Field>
            <FieldTitle>详细记录</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {raw && typeof raw === "object" && !Array.isArray(raw) && (
                <Collapsible>
                  <CollapsibleTrigger asChild>
                    <Button type="button" variant="ghost" size="sm">
                      查看详情
                      <ChevronDown />
                    </Button>
                  </CollapsibleTrigger>
                  <CollapsibleContent>
                    <NamedClientState
                      value={raw}
                      path={path}
                      section={`${section}[${(pagination.page - 1) * Number(pagination.size.value) + index}]`}
                    />
                  </CollapsibleContent>
                </Collapsible>
              )}
            </div>
          </Field>
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
