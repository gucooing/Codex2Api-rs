"use client";
import { ConsumerForm } from "@/components/forms/consumer-form";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useActions, useDialogFocus } from "@/lib/actions";
import type { SupplierQuotaWindow } from "@/lib/api";
import { type Consumer, type List } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { X } from "lucide-react";
import type { Dispatch, SetStateAction } from "react";

type Props = {
  actions: ReturnType<typeof useActions>;
  setCreate: Dispatch<SetStateAction<boolean>>;
  dialogFocus: ReturnType<typeof useDialogFocus>;
  resource: ReturnType<
    typeof useResource<
      List<Consumer & { quota: { windows: SupplierQuotaWindow[] } }> & {
        total: number;
        page: number;
        page_size: number;
      }
    >
  >;
};

export function CreateConsumerDialog({ actions, setCreate, dialogFocus, resource }: Props) {
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !actions.running.size) (() => setCreate(false))();
      }}
    >
      <DialogContent
        {...dialogFocus}
        showCloseButton={false}
        className="flex max-h-[90dvh] min-h-0 flex-col sm:max-w-3xl"
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (actions.running.size) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (actions.running.size) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{"创建虚拟账户"}</DialogTitle>
        </DialogHeader>
        <DialogClose asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="absolute right-4 top-4"
            aria-label="关闭"
            disabled={actions.running.size > 0}
          >
            <X />
          </Button>
        </DialogClose>
        <ConsumerForm
          onCancel={() => setCreate(false)}
          onSaved={() => {
            setCreate(false);
            resource.reload();
          }}
        />
      </DialogContent>
    </Dialog>
  );
}
