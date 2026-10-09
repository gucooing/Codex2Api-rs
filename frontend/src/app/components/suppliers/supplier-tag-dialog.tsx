"use client";
import { useSupplierTagsBatchDialog } from "@/app/data/suppliers";
import { SupplierTagEditor } from "@/components/forms/supplier-tags-form";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { type SupplierSelection } from "@/lib/supplier-selection";
export function SupplierTagsBatchDialog({
  open,
  onOpenChange,
  accounts,
  disabled,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  accounts: SupplierSelection[];
  disabled: boolean;
  onSaved: () => void;
}) {
  const { busy } = useSupplierTagsBatchDialog();
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy) onOpenChange(next);
      }}
    >
      <DialogContent
        className="sm:max-w-xl"
        onInteractOutside={(event) => {
          if (busy) event.preventDefault();
        }}
        onEscapeKeyDown={(event) => {
          if (busy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>批量更新标签</DialogTitle>
          <DialogDescription>已选择 {accounts.length} 个供应账户。</DialogDescription>
        </DialogHeader>
        <SupplierTagEditor
          key={accounts.map((account) => account.id).join(",")}
          accounts={accounts}
          disabled={disabled}
          batch
          onSaved={onSaved}
        />
      </DialogContent>
    </Dialog>
  );
}
