"use client";
import { ConsumerBatchDialogProps, useConsumerBatchDialog } from "@/app/data/consumers";
import { Button } from "@/components/ui/button";
import { CardDescription } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Field, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";

export function ConsumerBatchDialog({
  batchOpen,
  batchBusy,
  setBatchOpen,
  dialogFocus,
  batchDialog,
  actions,
  runBatch,
  resource,
  fieldId,
  grantQuantity,
  setGrantQuantity,
  grantStartMode,
  setGrantStartMode,
  grantActivateAt,
  setGrantActivateAt,
  grantDuration,
  setGrantDuration,
  grantNote,
  setGrantNote,
}: ConsumerBatchDialogProps) {
  const { handleSubmit } = useConsumerBatchDialog({ actions, runBatch });
  return (
    <Dialog
      open={batchOpen}
      onOpenChange={(open) => {
        if (!open && !batchBusy) setBatchOpen(false);
      }}
    >
      <DialogContent
        {...dialogFocus}
        showCloseButton={false}
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (batchBusy) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (batchBusy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>
            {batchDialog?.operation === "grant_reset"
              ? "发放重置卡"
              : batchDialog?.operation === "reset"
                ? "重置用量"
                : "删除虚拟账户"}
          </DialogTitle>
        </DialogHeader>
        <form noValidate onSubmit={(event) => handleSubmit(event)}>
          <FieldSet disabled={batchBusy || !resource.ready || !batchOpen} className="gap-3">
            <CardDescription>
              已选择 {batchDialog?.count ?? 0} 个账户
              {batchDialog?.selection.all_matching ? "（全部筛选结果）" : ""}
            </CardDescription>
            {batchDialog?.operation === "grant_reset" ? (
              <>
                <Field>
                  <FieldLabel htmlFor={`${fieldId}-batch-quantity`}>
                    发放数量（每个账户）
                  </FieldLabel>
                  <Input
                    id={`${fieldId}-batch-quantity`}
                    type="number"
                    min={1}
                    max={100}
                    step={1}
                    required
                    value={grantQuantity}
                    onChange={(e) => setGrantQuantity(e.target.value)}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${fieldId}-batch-mode`}>启用方式</FieldLabel>
                  <Select
                    value={grantStartMode}
                    onValueChange={setGrantStartMode}
                    disabled={batchBusy}
                  >
                    <SelectTrigger id={`${fieldId}-batch-mode`}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="now">立即启用</SelectItem>
                      <SelectItem value="scheduled">定时启用</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                {grantStartMode === "scheduled" && (
                  <Field>
                    <FieldLabel htmlFor={`${fieldId}-batch-start`}>启用时间</FieldLabel>
                    <Input
                      id={`${fieldId}-batch-start`}
                      type="datetime-local"
                      required
                      value={grantActivateAt}
                      onChange={(e) => setGrantActivateAt(e.target.value)}
                    />
                  </Field>
                )}
                <Field>
                  <FieldLabel htmlFor={`${fieldId}-batch-duration`}>有效时长（天）</FieldLabel>
                  <Input
                    id={`${fieldId}-batch-duration`}
                    type="number"
                    min={1}
                    max={3650}
                    step={1}
                    required
                    value={grantDuration}
                    onChange={(e) => setGrantDuration(e.target.value)}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${fieldId}-batch-note`}>管理备注</FieldLabel>
                  <Input
                    id={`${fieldId}-batch-note`}
                    maxLength={256}
                    value={grantNote}
                    onChange={(e) => setGrantNote(e.target.value)}
                  />
                </Field>
              </>
            ) : (
              <CardDescription>
                {batchDialog?.operation === "delete"
                  ? "删除所选账户并撤销登录？"
                  : "重置所选账户用量及周期？订阅到期时间不变。"}
              </CardDescription>
            )}
            <div className="flex justify-end gap-2">
              <Button type="button" variant="outline" onClick={() => setBatchOpen(false)}>
                取消
              </Button>
              <Button
                type="submit"
                variant={batchDialog?.operation === "delete" ? "destructive" : "default"}
                disabled={!batchDialog || batchDialog.count === 0 || !resource.ready || batchBusy}
              >
                {batchBusy && <Spinner />}确认
              </Button>
            </div>
          </FieldSet>
        </form>
      </DialogContent>
    </Dialog>
  );
}
