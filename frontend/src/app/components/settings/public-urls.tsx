"use client";
import { fields, usePublicUrls } from "@/app/data/settings";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { CircleAlert } from "lucide-react";

const descriptions = {
  api_url:
    "客户端连接 AI 服务时使用的对外地址，例如 https://api.example.com。填写实际可访问的协议、域名和可选端口，不需要填写 /v1 等路径。",
  user_url:
    "普通用户访问用户网站时使用的对外地址，例如 https://user.example.com。填写协议、域名和可选端口，不需要填写 /user 等路径。",
  admin_url:
    "管理员访问管理后台时使用的对外地址，例如 https://admin.example.com。填写协议、域名和可选端口，不需要填写 /admin 等路径。",
} as const;

export function PublicUrls() {
  const { fieldId, resource, setDraft, current, busy, disabled, handleSubmit } = usePublicUrls();
  return (
    <form
      noValidate
      className="space-y-4"
      aria-busy={busy || resource.refreshing}
      onSubmit={(event) => handleSubmit(event)}
    >
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={resource.reload}
          disabled={resource.refreshing || busy}
        >
          {resource.refreshing && <Spinner />}刷新
        </Button>
        <Button type="submit" size="sm" disabled={disabled}>
          {busy && <Spinner />}
          {busy ? "正在保存…" : "保存"}
        </Button>
      </div>
      <Card>
        <CardContent>
          <FieldSet>
            <FieldGroup className="gap-4">
              {fields.map((field) => (
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
                        {descriptions[field.key]}
                      </PopoverContent>
                    </Popover>
                  </div>
                  <Input
                    id={`${fieldId}-${field.key}`}
                    type="url"
                    disabled={disabled}
                    required
                    autoComplete="off"
                    spellCheck={false}
                    value={current?.[field.key] ?? ""}
                    onChange={(event) => {
                      if (current) setDraft({ ...current, [field.key]: event.target.value });
                    }}
                  />
                </Field>
              ))}
            </FieldGroup>
          </FieldSet>
        </CardContent>
      </Card>
    </form>
  );
}
