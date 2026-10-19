"use client";
import type { useApiKeysPage } from "@/app/data/api-keys";
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
import { apiLimit } from "@/lib/api-access";

export function ApiKeyEditorDialog({ state }: { state: ReturnType<typeof useApiKeysPage> }) {
  const {
    id,
    editing,
    setEditing,
    saving,
    submit,
    name,
    setName,
    groupId,
    setGroupId,
    enabled,
    setEnabled,
    groups,
    resource,
  } = state;
  const selectedGroup = groups.data?.items.find((group) => group.id === groupId);
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
          <DialogTitle>{editing ? "编辑密钥" : "创建 API 密钥"}</DialogTitle>
          <DialogDescription>调用费用从 USD 钱包按量扣除。</DialogDescription>
        </DialogHeader>
        <form noValidate className="space-y-4" onSubmit={submit}>
          <FieldSet disabled={saving || !groups.ready || !resource.ready}>
            <Field>
              <FieldLabel htmlFor={id + "-name"}>密钥名称</FieldLabel>
              <Input
                id={id + "-name"}
                value={name}
                maxLength={128}
                required
                autoComplete="off"
                onChange={(event) => setName(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor={id + "-group-editor"}>API 组</FieldLabel>
              <Select value={groupId} disabled={!!editing} onValueChange={setGroupId}>
                <SelectTrigger id={id + "-group-editor"} className="w-full">
                  <SelectValue placeholder={editing?.group_name ?? "请选择 API 组"} />
                </SelectTrigger>
                <SelectContent position="popper">
                  {editing &&
                    !groups.data?.items.some((group) => group.id === editing.group_id) && (
                      <SelectItem value={editing.group_id}>{editing.group_name}</SelectItem>
                    )}
                  {groups.data?.items.map((group) => (
                    <SelectItem key={group.id} value={group.id}>
                      {group.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            {editing && (
              <Field>
                <FieldLabel htmlFor={id + "-enabled"}>状态</FieldLabel>
                <Select
                  value={enabled ? "enabled" : "disabled"}
                  onValueChange={(status) => setEnabled(status === "enabled")}
                >
                  <SelectTrigger id={id + "-enabled"} className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="enabled">已启用</SelectItem>
                    <SelectItem value="disabled">已停用</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
            )}
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-sm">
              <dt className="text-muted-foreground">组倍率</dt>
              <dd>{selectedGroup?.multiplier ?? "—"}×</dd>
              <dt className="text-muted-foreground">每用户组并发</dt>
              <dd>{selectedGroup ? apiLimit(selectedGroup.concurrency_limit) : "—"}</dd>
              <dt className="text-muted-foreground">每用户组 RPM</dt>
              <dd>{selectedGroup ? apiLimit(selectedGroup.rpm_limit) : "—"}</dd>
            </dl>
          </FieldSet>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setEditing(undefined)}
              disabled={saving}
            >
              取消
            </Button>
            <Button type="submit" disabled={saving || !groups.ready || !resource.ready}>
              {saving && <Spinner />}
              {editing ? "保存" : "创建"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
