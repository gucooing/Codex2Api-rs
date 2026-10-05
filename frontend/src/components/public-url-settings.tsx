"use client";

import { useId, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type PublicUrlSettings } from "@/lib/api";
import { useResource } from "@/lib/hooks";

const fields = [
  {
    key: "api_url",
    label: "API 基础地址",
    description: "客户端实际访问的地址，用于登录授权、令牌交换和客户端服务链接。",
  },
  {
    key: "user_url",
    label: "用户端基础地址",
    description: "浏览器实际访问用户网站的地址，用于登录确认页和设备码授权页。",
  },
  {
    key: "admin_url",
    label: "管理端基础地址",
    description: "浏览器实际访问管理网站的地址，用于管理会话的安全设置。",
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
            <FieldDescription>
              填写完整基础地址，保留实际使用的 http:// 或 https://
              及端口。反向代理部署时填写对外地址，服务自动添加各端路径。保存后立即生效。
            </FieldDescription>
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
                  <FieldDescription>{field.description}</FieldDescription>
                </Field>
              ))}
            </FieldGroup>
          </FieldSet>
        </CardContent>
      </Card>
    </form>
  );
}
