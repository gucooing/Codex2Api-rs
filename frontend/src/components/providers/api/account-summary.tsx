"use client";
import { Badge } from "@/components/ui/badge";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
import type { Fingerprint, ProxyOption } from "@/lib/api";
import { apiAuthLabel, apiWireLabel } from "@/lib/providers/api/channel";
import { useId } from "react";
import { ApiKeyField } from "./key-field";

/** Everything a new API-key account is created with, for the administrator to confirm. */
export function ApiAccountSummary({
  config,
  apiKey,
  models,
  proxies,
}: {
  config: Fingerprint | undefined;
  apiKey: string;
  models: string[];
  proxies: ProxyOption[];
}) {
  const id = useId();
  const statuses = config?.retry_statuses ?? [];
  const proxy = proxies.find((item) => item.id === config?.proxy_id);
  const rows = [
    { label: "提供商", value: "API" },
    { label: "名称", value: config?.name?.trim() },
    { label: "上游线格式", value: apiWireLabel(config?.wire) },
    { label: "认证头", value: apiAuthLabel(config?.auth_scheme) },
    { label: "Base URL", value: config?.base_url?.trim() },
    {
      label: "出站代理",
      value: proxy ? `${proxy.name} · ${proxy.display_url}` : "不使用代理",
    },
  ];
  return (
    <FieldGroup className="gap-4">
      <FieldGroup className="grid gap-3 sm:grid-cols-2">
        {rows.map(({ label, value }) => (
          <Field key={label}>
            <FieldTitle>{label}</FieldTitle>
            <FieldDescription className="break-all">{value || "—"}</FieldDescription>
          </Field>
        ))}
      </FieldGroup>
      <ApiKeyField id={`${id}-key`} value={apiKey.trim()} readOnly />
      <Field>
        <FieldTitle>同号重试</FieldTitle>
        <div className="flex flex-wrap items-center gap-1.5">
          {statuses.map((status) => (
            <Badge key={status} variant="secondary">
              {status}
            </Badge>
          ))}
          <span className="text-sm text-muted-foreground">
            {statuses.length ? `${config?.retry_count ?? "—"} 次` : "未启用"}
          </span>
        </div>
      </Field>
      <Field>
        <FieldTitle>模型（{models.length}）</FieldTitle>
        <div className="flex flex-wrap gap-1.5">
          {models.map((model) => (
            <Badge key={model} variant="secondary" className="max-w-full truncate" title={model}>
              {model}
            </Badge>
          ))}
        </div>
      </Field>
    </FieldGroup>
  );
}
