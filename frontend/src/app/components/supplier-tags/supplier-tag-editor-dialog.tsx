"use client";
import { SupplierTagEditorDialogProps, useSupplierTagEditorDialog } from "@/app/data/supplier-tags";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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

export function SupplierTagEditorDialog({
  editing,
  saving,
  setEditing,
  actions,
  tags,
  name,
  provider,
  id,
  setName,
  setProvider,
}: SupplierTagEditorDialogProps) {
  const { handleSubmit } = useSupplierTagEditorDialog({
    editing,
    setEditing,
    actions,
    tags,
    name,
    provider,
  });
  return (
    <Dialog
      open={editing !== undefined}
      onOpenChange={(open) => {
        if (!open && !saving) setEditing(undefined);
      }}
    >
      <DialogContent
        showCloseButton={!saving}
        onEscapeKeyDown={(event) => {
          if (saving) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (saving) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{editing ? "编辑标签" : "添加标签"}</DialogTitle>
          <DialogDescription>设置标签名称和所属平台。</DialogDescription>
        </DialogHeader>
        <form noValidate className="space-y-4" onSubmit={(event) => handleSubmit(event)}>
          <FieldSet disabled={saving || !tags.ready}>
            <Field>
              <FieldLabel htmlFor={`${id}-name`}>标签名称</FieldLabel>
              <Input
                id={`${id}-name`}
                autoComplete="off"
                maxLength={80}
                required
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor={`${id}-edit-platform`}>平台</FieldLabel>
              <Select value={provider} disabled={!!editing} onValueChange={setProvider}>
                <SelectTrigger id={`${id}-edit-platform`} className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="chatgpt">ChatGPT</SelectItem>
                  <SelectItem value="grok">Grok</SelectItem>
                </SelectContent>
              </Select>
            </Field>
          </FieldSet>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={saving}
              onClick={() => setEditing(undefined)}
            >
              取消
            </Button>
            <Button type="submit" disabled={saving || !tags.ready}>
              {saving && <Spinner />}保存
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
