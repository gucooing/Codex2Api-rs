"use client";
import { useSecurityEditor } from "@/app/data/settings";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Spinner } from "@/components/ui/spinner";
import { CircleAlert } from "lucide-react";

const groups = [
  {
    title: "验证当前身份",
    fields: [
      {
        key: "old_username",
        label: "当前用户名",
        type: "text",
        autoComplete: "username",
        required: true,
        description: "填写现在登录管理后台使用的用户名，用来确认本次修改的身份。",
      },
      {
        key: "old_password",
        label: "当前密码",
        type: "password",
        autoComplete: "current-password",
        required: true,
        description: "填写现在登录管理后台使用的密码。验证通过后，才能修改登录凭据。",
      },
    ],
  },
  {
    title: "更新登录凭据",
    fields: [
      {
        key: "new_username",
        label: "新用户名",
        type: "text",
        autoComplete: "username",
        required: true,
        description: "以后登录管理后台使用的用户名。保持原值就不修改用户名；保存后需要重新登录。",
      },
      {
        key: "new_password",
        label: "新密码",
        type: "password",
        autoComplete: "new-password",
        required: false,
        description: "以后登录管理后台使用的密码。留空会保留原密码；保存后需要重新登录。",
      },
    ],
  },
] as const;

export function SecurityEditor({
  username,
  disabled,
  refreshing,
  onReload,
}: {
  username: string;
  disabled: boolean;
  refreshing: boolean;
  onReload: () => void;
}) {
  const { fieldId, form, update, busy, handleSubmit } = useSecurityEditor({ username, disabled });
  return (
    <form noValidate className="space-y-4" aria-busy={busy || refreshing} onSubmit={handleSubmit}>
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onReload}
          disabled={refreshing || busy}
        >
          {refreshing && <Spinner />}刷新
        </Button>
        <Button type="submit" size="sm" disabled={disabled || busy}>
          {busy && <Spinner />}
          {busy ? "正在保存…" : "保存"}
        </Button>
      </div>
      {groups.map((group) => (
        <Card key={group.title}>
          <CardContent>
            <FieldSet>
              <FieldLegend variant="label">{group.title}</FieldLegend>
              <FieldGroup className="grid gap-4 sm:grid-cols-2">
                {group.fields.map((field) => (
                  <Field key={field.key}>
                    <div className="flex items-center gap-1">
                      <FieldLabel htmlFor={`${fieldId}-${field.key}`}>{field.label}</FieldLabel>
                      <Popover>
                        <PopoverTrigger asChild>
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            className="size-5 text-muted-foreground"
                            aria-label={`${field.label}说明`}
                          >
                            <CircleAlert className="size-3" aria-hidden="true" />
                          </Button>
                        </PopoverTrigger>
                        <PopoverContent
                          align="start"
                          collisionPadding={16}
                          aria-label={`${field.label}说明`}
                        >
                          {field.description}
                        </PopoverContent>
                      </Popover>
                    </div>
                    <Input
                      id={`${fieldId}-${field.key}`}
                      type={field.type}
                      autoComplete={field.autoComplete}
                      required={field.required}
                      value={form[field.key]}
                      onChange={(event) => update(field.key, event.target.value)}
                      disabled={disabled || busy}
                    />
                  </Field>
                ))}
              </FieldGroup>
            </FieldSet>
          </CardContent>
        </Card>
      ))}
    </form>
  );
}
