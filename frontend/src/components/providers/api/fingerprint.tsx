"use client";
import { useApiRetryStatuses } from "@/app/data/suppliers/api-accounts";
import {
  Combobox,
  ComboboxChip,
  ComboboxChips,
  ComboboxChipsInput,
  ComboboxValue,
} from "@/components/ui/combobox";
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
import {
  apiAuthSchemeOptions,
  apiNameMax,
  apiRetryCountMax,
  apiWireOptions,
} from "@/lib/providers/api/channel";
import { useId } from "react";

/** The name and endpoint configuration of an API-key account; the key is entered apart. */
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
  const wire = value.wire ?? "";
  const authOptions = apiAuthSchemeOptions(wire);
  const retry = useApiRetryStatuses(value, onChange);
  const selectWire = (next: string) => {
    const option = apiWireOptions.find((item) => item.value === next);
    const previous = apiWireOptions.find((item) => item.value === wire);
    const schemes = apiAuthSchemeOptions(next).map((item) => item.value);
    // Follow the vendor's own scheme unless the administrator chose another one the
    // new wire also accepts.
    const chosen =
      value.auth_scheme &&
      value.auth_scheme !== authOptions[0]?.value &&
      schemes.includes(value.auth_scheme);
    onChange({
      ...value,
      wire: next,
      base_url:
        option && (!value.base_url || value.base_url === previous?.baseUrl)
          ? option.baseUrl
          : value.base_url,
      auth_scheme: chosen ? value.auth_scheme : schemes[0],
    });
  };
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <Field className="sm:col-span-2">
        <FieldLabel htmlFor={`${id}-name`}>名称</FieldLabel>
        <Input
          id={`${id}-name`}
          required
          maxLength={apiNameMax}
          autoComplete="off"
          value={value.name ?? ""}
          onChange={(event) => onChange({ ...value, name: event.target.value })}
        />
      </Field>
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
          value={value.auth_scheme ?? ""}
          disabled={authOptions.length < 2}
          onValueChange={(auth_scheme) => onChange({ ...value, auth_scheme })}
        >
          <SelectTrigger id={`${id}-auth`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            {authOptions.map((option) => (
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
        <Combobox
          multiple
          open={false}
          value={retry.statuses}
          onValueChange={retry.change}
          inputValue={retry.input}
          onInputValueChange={retry.setInput}
        >
          <ComboboxChips>
            <ComboboxValue>
              {(statuses: number[]) =>
                statuses.map((status) => <ComboboxChip key={status}>{status}</ComboboxChip>)
              }
            </ComboboxValue>
            <ComboboxChipsInput
              id={`${id}-retry`}
              inputMode="numeric"
              autoComplete="off"
              placeholder="输入状态码后按回车"
              onKeyDown={retry.keyDown}
            />
          </ComboboxChips>
        </Combobox>
        <FieldDescription>遇到这些 HTTP 状态时先在本账户重试，再换号</FieldDescription>
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-retry-count`}>同号重试次数</FieldLabel>
        <Input
          id={`${id}-retry-count`}
          type="number"
          min={1}
          max={apiRetryCountMax}
          step={1}
          required
          disabled={retry.statuses.length === 0}
          value={value.retry_count ?? ""}
          onChange={(event) =>
            onChange({
              ...value,
              retry_count: event.target.value === "" ? undefined : Number(event.target.value),
            })
          }
        />
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
    </div>
  );
}
