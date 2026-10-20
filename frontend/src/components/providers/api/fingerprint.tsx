"use client";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { Fingerprint, ProxyOption } from "@/lib/api";
import { apiAuthSchemeOptions, apiWireOptions } from "@/lib/providers/api/channel";
import { useId } from "react";

/** The endpoint configuration of an API-key account; the key itself is entered apart. */
export function ApiFingerprintFields({
  value,
  onChange,
  proxies,
}: {
  value: Fingerprint;
  onChange: (value: Fingerprint) => void;
  proxies: ProxyOption[];
}) {
  const id = useId();
  const wire = value.wire ?? "chat_completions";
  const selectWire = (next: string) => {
    const option = apiWireOptions.find((item) => item.value === next);
    const previous = apiWireOptions.find((item) => item.value === wire);
    onChange({
      ...value,
      wire: next,
      base_url:
        option && (!value.base_url || value.base_url === previous?.baseUrl)
          ? option.baseUrl
          : value.base_url,
      auth_scheme: option && value.auth_scheme === previous?.auth ? option.auth : value.auth_scheme,
    });
  };
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <Field>
        <FieldLabel htmlFor={`${id}-wire`}>上游线格式</FieldLabel>
        <Select value={wire} onValueChange={selectWire}>
          <SelectTrigger id={`${id}-wire`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            {apiWireOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-auth`}>认证头</FieldLabel>
        <Select
          value={value.auth_scheme ?? "bearer"}
          onValueChange={(auth_scheme) => onChange({ ...value, auth_scheme })}
        >
          <SelectTrigger id={`${id}-auth`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            {apiAuthSchemeOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <Field className="sm:col-span-2">
        <FieldLabel htmlFor={`${id}-base-url`}>Base URL</FieldLabel>
        <Input
          id={`${id}-base-url`}
          required
          autoComplete="off"
          spellCheck={false}
          value={value.base_url ?? ""}
          onChange={(event) => onChange({ ...value, base_url: event.target.value })}
        />
        <FieldDescription>
          请求发往 Base URL 下的 responses、chat/completions 或 messages
        </FieldDescription>
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-retry`}>同号重试状态码</FieldLabel>
        <Input
          id={`${id}-retry`}
          autoComplete="off"
          placeholder="例如 500, 502"
          value={(value.retry_statuses ?? []).join(", ")}
          onChange={(event) =>
            onChange({
              ...value,
              retry_statuses: event.target.value
                .split(/[\s,，]+/)
                .filter(Boolean)
                .map(Number)
                .filter((status) => Number.isInteger(status)),
            })
          }
        />
        <FieldDescription>遇到这些 HTTP 状态时先在本账户重试两次再换号</FieldDescription>
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-proxy`}>出站代理</FieldLabel>
        <Select
          value={value.proxy_id ?? "none"}
          onValueChange={(proxy_id) =>
            onChange({ ...value, proxy_id: proxy_id === "none" ? null : proxy_id })
          }
        >
          <SelectTrigger id={`${id}-proxy`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            <SelectItem value="none">不使用代理</SelectItem>
            {proxies.map((p) => (
              <SelectItem key={p.id} value={p.id}>
                {p.name} · {p.display_url}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      {value.key_suffix && (
        <Field>
          <FieldLabel htmlFor={`${id}-key`}>当前 Key</FieldLabel>
          <Input id={`${id}-key`} readOnly value={`…${value.key_suffix}`} />
          <FieldDescription>更换 Key 请使用“重新授权”</FieldDescription>
        </Field>
      )}
    </div>
  );
}
