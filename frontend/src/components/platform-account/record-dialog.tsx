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
import { ChevronRight } from "lucide-react";

type Props = {
  cell: { label: string; text: string; status: boolean; failed: boolean };
  row: {
    key: string | number;
    cells: { label: string; text: string; status: boolean; failed: boolean }[];
  };
};

export function RecordDialog({ cell, row }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
          aria-label={"查看详情：" + String(cell.text)}
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">{cell.text}</span>
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
          {row.cells.map((cell) => (
            <Field key={cell.label}>
              <FieldTitle>{cell.label}</FieldTitle>
              <div className="min-w-0 break-words">
                {cell.status ? (
                  <Badge variant={cell.failed ? "destructive" : "secondary"}>{cell.text}</Badge>
                ) : (
                  cell.text
                )}
              </div>
            </Field>
          ))}
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
