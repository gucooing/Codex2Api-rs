"use client";

import { useId, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type PublicUrlSettings } from "@/lib/api";
import { useResource } from "@/lib/hooks";

const fields = [
  {
    key: "api_url",
    label: "API 基础地址",
  },
  {
    key: "user_url",
    label: "用户端基础地址",
  },
  {
    key: "admin_url",
    label: "管理端基础地址",
  },
] as const;

export function PublicUrls() {
  const fieldId = useId();
  const resource = useResource<PublicUrlSettings>("/settings/public-urls");
  const actions = useActions();
  const [draft, setDraft] = useState<PublicUrlSettings>();
  const [committed, setCommitted] = useState<PublicUrlSettings>();
  useErrorToast(resource.error);
  const loaded =
    committed && (!resource.data || committed.revision > resource.data.revision)
      ? committed
      : resource.data;
  const current = draft ?? loaded;
  const busy = actions.isBusy("settings:public-urls");
  const disabled = !resource.ready || !current || busy;

  return (
    <form
      noValidate
      className="space-y-4"
      aria-busy={busy || resource.refreshing}
      onSubmit={(event) =>
        actions.submit(
          event,
          "settings:public-urls",
          async () => {
            if (!resource.ready || !current) throw new Error("请先加载访问地址");
            const input = { ...current };
            for (const field of fields) {
              const value = input[field.key].trim();
              let url: URL;
              try {
                url = new URL(value);
              } catch {
                throw new Error(`${field.label}须包含 http:// 或 https:// 和域名`);
              }
              if (
                !["http:", "https:"].includes(url.protocol) ||
                !url.hostname ||
                url.username ||
                url.password ||
                url.pathname !== "/" ||
                value.includes("?") ||
                value.includes("#") ||
                value.includes("\\") ||
                !value.includes("://") ||
                /\s/.test(value)
              )
                throw new Error(`${field.label}仅填写协议、域名和可选端口，不含路径或参数`);
              input[field.key] = url.origin;
            }
            const saved = await request<PublicUrlSettings>("/settings/public-urls", {
              method: "PUT",
              body: input,
            });
            setCommitted(saved);
            setDraft(undefined);
            resource.reload();
          },
          "访问地址已保存并生效",
        )
      }
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
          <FieldSet disabled={disabled}>
            <FieldGroup className="gap-4">
              {fields.map((field) => (
                <Field key={field.key}>
                  <FieldLabel htmlFor={`${fieldId}-${field.key}`}>{field.label}</FieldLabel>
                  <Input
                    id={`${fieldId}-${field.key}`}
                    type="url"
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
