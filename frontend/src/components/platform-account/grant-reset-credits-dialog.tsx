"use client";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { GrantResetCreditsDialogProps, useGrantResetCreditsDialog } from "@/lib/platform-account";

export function GrantResetCreditsDialog({
  grantOpen,
  busy,
  setGrantOpen,
  dialogFocus,
  actions,
  id,
  path,
  quantity,
  note,
  startMode,
  activateAt,
  durationDays,
  setNote,
  resource,
  fieldId,
  setQuantity,
  setStartMode,
  setActivateAt,
  setDurationDays,
}: GrantResetCreditsDialogProps) {
  const { handleSubmit } = useGrantResetCreditsDialog({
    setGrantOpen,
    actions,
    id,
    path,
    quantity,
    note,
    startMode,
    activateAt,
    durationDays,
    setNote,
    resource,
  });
  return (
    <Dialog
      open={grantOpen}
      onOpenChange={(open) => {
        if (!busy) setGrantOpen(open);
      }}
    >
      <DialogContent
        {...dialogFocus}
        showCloseButton={false}
        aria-describedby={undefined}
        onEscapeKeyDown={(e) => {
          if (busy) e.preventDefault();
        }}
        onInteractOutside={(e) => {
          if (busy) e.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>发放重置卡</DialogTitle>
        </DialogHeader>
        <form noValidate className="grid gap-3" onSubmit={(event) => handleSubmit(event)}>
          <Field>
            <FieldLabel htmlFor={`${fieldId}-quantity`}>发放数量</FieldLabel>
            <Input
              id={`${fieldId}-quantity`}
              type="number"
              min={1}
              max={100}
              step={1}
              required
              value={quantity}
              onChange={(event) => setQuantity(event.target.value)}
              disabled={!resource.ready || busy}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor={`${fieldId}-note`}>管理备注</FieldLabel>
            <Input
              id={`${fieldId}-note`}
              maxLength={256}
              value={note}
              onChange={(event) => setNote(event.target.value)}
              disabled={!resource.ready || busy}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor={`${fieldId}-start-mode`}>启用方式</FieldLabel>
            <Select
              value={startMode}
              onValueChange={setStartMode}
              disabled={!resource.ready || busy}
            >
              <SelectTrigger id={`${fieldId}-start-mode`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="now">立即启用</SelectItem>
                <SelectItem value="scheduled">定时启用</SelectItem>
              </SelectContent>
            </Select>
          </Field>
          {startMode === "scheduled" && (
            <Field>
              <FieldLabel htmlFor={`${fieldId}-activate`}>启用时间</FieldLabel>
              <Input
                id={`${fieldId}-activate`}
                type="datetime-local"
                required
                value={activateAt}
                onChange={(event) => setActivateAt(event.target.value)}
                disabled={!resource.ready || busy}
              />
            </Field>
          )}
          <Field>
            <FieldLabel htmlFor={`${fieldId}-duration`}>有效时长（天）</FieldLabel>
            <Input
              id={`${fieldId}-duration`}
              type="number"
              min={1}
              max={3650}
              step={1}
              required
              value={durationDays}
              onChange={(e) => setDurationDays(e.target.value)}
              disabled={!resource.ready || busy}
            />
          </Field>
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => setGrantOpen(false)}
            >
              取消
            </Button>
            <Button type="submit" disabled={!resource.ready || busy}>
              {busy && <Spinner />}确认发放
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
