"use client";
import { UserEditorDialogProps, useUserEditorDialog } from "@/app/data/users";
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
import { Switch } from "@/components/ui/switch";

export function UserEditorDialog({
  editing,
  actions,
  setEditing,
  focus,
  resource,
  id,
}: UserEditorDialogProps) {
  const { handleSubmit } = useUserEditorDialog({ editing, actions, setEditing, resource });
  return (
    <Dialog
      open={!!editing}
      onOpenChange={(open) => {
        if (!open && !actions.isBusy("save-user")) setEditing(undefined);
      }}
    >
      <DialogContent {...focus}>
        <DialogHeader>
          <DialogTitle>{editing?.id ? "编辑用户" : "创建用户"}</DialogTitle>
          <DialogDescription>
            停用或修改密码会撤销用户网页会话及所有平台设备登录。
          </DialogDescription>
        </DialogHeader>
        <form noValidate onSubmit={(e) => handleSubmit(e)}>
          <FieldSet disabled={!editing || !resource.ready || actions.isBusy("save-user")}>
            <FieldGroup>
              {(
                [
                  ["username", "用户名"],
                  ["name", "名称"],
                  ["email", "邮箱"],
                  ["password", editing?.id ? "新密码（留空保留）" : "密码"],
                ] as const
              ).map(([key, label]) => (
                <Field key={key}>
                  <FieldLabel htmlFor={`${id}-${key}`}>{label}</FieldLabel>
                  <Input
                    id={`${id}-${key}`}
                    type={key === "password" ? "password" : key === "email" ? "email" : "text"}
                    autoComplete={key === "password" ? "new-password" : "off"}
                    required={key !== "password" || !editing?.id}
                    value={editing?.[key] ?? ""}
                    onChange={(e) => setEditing((v) => (v ? { ...v, [key]: e.target.value } : v))}
                  />
                </Field>
              ))}
              <Field orientation="horizontal">
                <FieldLabel htmlFor={`${id}-enabled`}>允许登录</FieldLabel>
                <Switch
                  id={`${id}-enabled`}
                  checked={editing?.enabled ?? false}
                  onCheckedChange={(enabled) => setEditing((v) => (v ? { ...v, enabled } : v))}
                />
              </Field>
              <DialogFooter>
                <Button type="submit">保存</Button>
              </DialogFooter>
            </FieldGroup>
          </FieldSet>
        </form>
      </DialogContent>
    </Dialog>
  );
}
