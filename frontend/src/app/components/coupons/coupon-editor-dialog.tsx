"use client";
import { CouponEditorDialogProps, useCouponEditorDialog } from "@/app/data/coupons";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

export function CouponEditorDialog({
  edit,
  busy,
  setEdit,
  actions,
  resource,
  plans,
  id,
  update,
}: CouponEditorDialogProps) {
  const { handleSubmit } = useCouponEditorDialog({ edit, setEdit, actions, resource, plans });
  return (
    <Dialog
      open={!!edit}
      onOpenChange={(open) => {
        if (!open && !busy) setEdit(undefined);
      }}
    >
      <DialogContent
        aria-describedby={undefined}
        className="flex max-h-[90dvh] flex-col sm:max-w-xl"
      >
        <DialogHeader>
          <DialogTitle>{edit?.id ? "编辑优惠券" : "创建优惠券"}</DialogTitle>
        </DialogHeader>
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          onSubmit={(event) => handleSubmit(event)}
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-15rem)]">
            <div className="grid gap-4 pr-3 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor={`${id}-code`}>优惠码</FieldLabel>
                <Input
                  id={`${id}-code`}
                  value={edit?.code ?? ""}
                  onChange={(e) => update({ code: e.target.value })}
                  required
                  maxLength={64}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-name`}>名称</FieldLabel>
                <Input
                  id={`${id}-name`}
                  value={edit?.name ?? ""}
                  onChange={(e) => update({ name: e.target.value })}
                  required
                  maxLength={120}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-discount`}>优惠金额（USD）</FieldLabel>
                <Input
                  id={`${id}-discount`}
                  inputMode="decimal"
                  value={edit?.discount ?? ""}
                  onChange={(e) => update({ discount: e.target.value })}
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-minimum`}>最低应付金额（USD）</FieldLabel>
                <Input
                  id={`${id}-minimum`}
                  inputMode="decimal"
                  value={edit?.minimum ?? ""}
                  onChange={(e) => update({ minimum: e.target.value })}
                  required
                />
              </Field>
              <Field className="sm:col-span-2">
                <FieldLabel htmlFor={`${id}-plan`}>适用套餐</FieldLabel>
                <Select value={edit?.plan ?? "all"} onValueChange={(plan) => update({ plan })}>
                  <SelectTrigger id={`${id}-plan`}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="all">全部付费套餐</SelectItem>
                    {plans.data?.items.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-starts`}>开始时间</FieldLabel>
                <Input
                  id={`${id}-starts`}
                  type="datetime-local"
                  value={edit?.starts ?? ""}
                  onChange={(e) => update({ starts: e.target.value })}
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-ends`}>结束时间</FieldLabel>
                <Input
                  id={`${id}-ends`}
                  type="datetime-local"
                  value={edit?.ends ?? ""}
                  onChange={(e) => update({ ends: e.target.value })}
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-max`}>总使用次数（留空不限）</FieldLabel>
                <Input
                  id={`${id}-max`}
                  inputMode="numeric"
                  value={edit?.max ?? ""}
                  onChange={(e) => update({ max: e.target.value })}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-per`}>每人使用次数</FieldLabel>
                <Input
                  id={`${id}-per`}
                  inputMode="numeric"
                  value={edit?.perUser ?? ""}
                  onChange={(e) => update({ perUser: e.target.value })}
                  required
                />
              </Field>
              <Field orientation="horizontal">
                <FieldLabel htmlFor={`${id}-enabled`}>启用</FieldLabel>
                <Switch
                  id={`${id}-enabled`}
                  checked={edit?.enabled ?? false}
                  onCheckedChange={(enabled) => update({ enabled })}
                />
              </Field>
            </div>
          </ScrollArea>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => setEdit(undefined)}
            >
              取消
            </Button>
            <Button disabled={busy || !resource.ready || !plans.ready}>保存</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
