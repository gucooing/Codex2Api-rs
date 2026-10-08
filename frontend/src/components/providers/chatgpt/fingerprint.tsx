"use client";

import { Button } from "@/components/ui/button";

import { Field, FieldLabel } from "@/components/ui/field";
import { useId } from "react";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";

import { useActions } from "@/lib/actions";

import { request, type Fingerprint, type ProxyOption } from "@/lib/api";

export function ChatgptFingerprintFields({
  value,
  onChange,
  proxies,
}: {
  value: Fingerprint;
  onChange: (value: Fingerprint) => void;
  proxies: ProxyOption[];
}) {
  const fieldId = useId();
  const actions = useActions();
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      {(
        [
          ["os_type", "操作系统"],
          ["os_version", "系统版本"],
          ["arch", "架构"],
          ["terminal", "终端标识"],
        ] as const
      ).map(([key, label], fieldIndex7) => (
        <Field key={key}>
          <FieldLabel
            htmlFor={
              fieldId +
              "-field-8" +
              "-" +
              String(fieldIndex7) +
              "-" +
              encodeURIComponent(String(label))
            }
          >
            {label}
          </FieldLabel>
          <Input
            id={
              fieldId +
              "-field-8" +
              "-" +
              String(fieldIndex7) +
              "-" +
              encodeURIComponent(String(label))
            }
            aria-label={label}
            required
            maxLength={key === "terminal" ? 256 : 128}
            value={value[key]}
            onChange={(e) => onChange({ ...value, [key]: e.target.value })}
          />
        </Field>
      ))}
      <Field>
        <FieldLabel htmlFor={fieldId + "-field-9" + "-" + encodeURIComponent(String("出站代理"))}>
          {"出站代理"}
        </FieldLabel>
        <Select
          value={value.proxy_id ?? ""}
          onValueChange={(next) =>
            ((proxy_id) => onChange({ ...value, proxy_id: proxy_id || null }))(
              next ===
                fieldId + "-field-9" + "-" + encodeURIComponent(String("出站代理")) + "-empty"
                ? ""
                : next,
            )
          }
        >
          <SelectTrigger
            id={fieldId + "-field-9" + "-" + encodeURIComponent(String("出站代理"))}
            aria-label={"出站代理"}
            data-required={false ? "true" : undefined}
            data-empty={String(value.proxy_id ?? "") === "" ? "true" : undefined}
            className="w-full"
          >
            <SelectValue
              placeholder={
                [
                  { value: "", label: "不使用代理" },
                  ...proxies.map((proxy) => ({
                    value: proxy.id,
                    label: `${proxy.name} · ${proxy.display_url}`,
                  })),
                ].find((option) => option.value === "")?.label ?? "请选择"
              }
            />
          </SelectTrigger>
          <SelectContent position="popper">
            {[
              { value: "", label: "不使用代理" },
              ...proxies.map((proxy) => ({
                value: proxy.id,
                label: `${proxy.name} · ${proxy.display_url}`,
              })),
            ].map((option) => (
              <SelectItem
                key={option.value}
                value={
                  option.value ||
                  fieldId + "-field-9" + "-" + encodeURIComponent(String("出站代理")) + "-empty"
                }
                disabled={"disabled" in option && Boolean(option.disabled)}
              >
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <Field>
        <FieldLabel htmlFor={fieldId + "-field-10" + "-" + encodeURIComponent(String("时区"))}>
          {"时区"}
        </FieldLabel>
        <div className="flex items-center gap-2">
          <Input
            id={fieldId + "-field-10" + "-" + encodeURIComponent(String("时区"))}
            aria-label={"时区"}
            value={value.timezone}
            onChange={(e) => onChange({ ...value, timezone: e.target.value })}
            placeholder="Asia/Taipei"
          />
          {value.proxy_id && proxies.some((proxy) => proxy.id === value.proxy_id) && (
            <Button
              type="button"
              variant={false ? "destructive" : "outline"}
              className="shrink-0"
              disabled={false || actions.isBusy("components\\suppliers.tsx:action:11")}
              onClick={() =>
                void actions.run(
                  "components\\suppliers.tsx:action:11",
                  async () => {
                    const result = await request<{ timezone: string }>(
                      `/proxies/${value.proxy_id}/check/timezone`,
                      { method: "POST", body: {} },
                    );
                    onChange({ ...value, timezone: result.timezone });
                  },
                  { confirm: undefined, danger: false, success: undefined },
                )
              }
            >
              应用代理时区
            </Button>
          )}
        </div>
      </Field>
    </div>
  );
}
