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
import { date } from "@/lib/format";
import { ChevronRight } from "lucide-react";

type Props = {
  format: "date" | undefined;
  item: Record<string, unknown>;
  field: string;
  title: string;
  fields: [string, ("date" | undefined)?][];
  columns: string[];
};

export function ResourceRecordDialog({ format, item, field, title, fields, columns }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 md:hidden"
          aria-label="查看记录详情"
        >
          <span className="min-w-0 flex-1 truncate">
            {format === "date"
              ? date(
                  typeof item[field] === "number" || typeof item[field] === "string"
                    ? (item[field] as string | number)
                    : null,
                )
              : item[field] == null
                ? "—"
                : String(item[field])}
          </span>
          <ChevronRight className="size-3 shrink-0" />
        </Button>
      </DialogTrigger>
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>当前记录的完整字段</DialogDescription>
        </DialogHeader>
        <FieldGroup className="gap-3">
          {fields.map(([field, format], fieldIndex) => (
            <Field key={field}>
              <FieldTitle>{columns[fieldIndex]}</FieldTitle>
              <div className="min-w-0 break-words">
                {format === "date"
                  ? date(
                      typeof item[field] === "number" || typeof item[field] === "string"
                        ? (item[field] as string | number)
                        : null,
                    )
                  : item[field] == null
                    ? "—"
                    : String(item[field])}
              </div>
            </Field>
          ))}
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
