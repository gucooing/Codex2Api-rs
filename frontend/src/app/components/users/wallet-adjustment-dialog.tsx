"use client";
import { WalletAdjustmentDialogProps, useWalletAdjustmentDialog } from "@/app/data/users";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { money } from "@/lib/format";

export function WalletAdjustmentDialog({
  adjusting,
  actions,
  setAdjusting,
  resource,
  amount,
  delta,
  after,
  before,
  id,
}: WalletAdjustmentDialogProps) {
  const { handleSubmit } = useWalletAdjustmentDialog({
    adjusting,
    actions,
    setAdjusting,
    resource,
    amount,
    delta,
    after,
  });
  return (
    <Dialog
      open={!!adjusting}
      onOpenChange={(open) => {
        if (!open && !actions.isBusy("adjust-wallet")) setAdjusting(undefined);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>调整余额</DialogTitle>
          <DialogDescription>{adjusting?.user.username ?? "用户钱包"}</DialogDescription>
        </DialogHeader>
        <form noValidate onSubmit={(event) => handleSubmit(event)}>
          <FieldSet disabled={actions.isBusy("adjust-wallet")}>
            <FieldGroup>
              <div className="grid grid-cols-2 gap-3 text-sm">
                <p>
                  当前余额{" "}
                  <span className="font-medium">
                    {before === null ? "—" : money(before / 100)} USD
                  </span>
                </p>
                <p>
                  调整后{" "}
                  <span className="font-medium">
                    {after === null || !Number.isSafeInteger(after) || after < 0
                      ? "—"
                      : money(after / 100)}{" "}
                    USD
                  </span>
                </p>
              </div>
              <Field>
                <FieldLabel htmlFor={`${id}-wallet-direction`}>操作</FieldLabel>
                <Select
                  value={adjusting?.direction ?? "increase"}
                  onValueChange={(direction) =>
                    setAdjusting((v) =>
                      v ? { ...v, direction: direction as "increase" | "decrease" } : v,
                    )
                  }
                >
                  <SelectTrigger id={`${id}-wallet-direction`}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="increase">增加余额</SelectItem>
                    <SelectItem value="decrease">减少余额</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-wallet-amount`}>金额（USD）</FieldLabel>
                <Input
                  id={`${id}-wallet-amount`}
                  inputMode="decimal"
                  value={adjusting?.amount ?? ""}
                  onChange={(e) => setAdjusting((v) => (v ? { ...v, amount: e.target.value } : v))}
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-wallet-reason`}>调整原因（选填）</FieldLabel>
                <Input
                  id={`${id}-wallet-reason`}
                  value={adjusting?.reason ?? ""}
                  onChange={(e) => setAdjusting((v) => (v ? { ...v, reason: e.target.value } : v))}
                  maxLength={300}
                />
              </Field>
              <DialogFooter>
                <Button type="button" variant="outline" onClick={() => setAdjusting(undefined)}>
                  取消
                </Button>
                <Button type="submit" disabled={!resource.ready || !adjusting}>
                  确认{adjusting?.direction === "decrease" ? "减少" : "增加"}
                </Button>
              </DialogFooter>
            </FieldGroup>
          </FieldSet>
        </form>
      </DialogContent>
    </Dialog>
  );
}
