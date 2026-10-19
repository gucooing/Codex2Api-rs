"use client";
import { useId } from "react";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Input } from "@/components/ui/input";
import type { Fingerprint, ProxyOption } from "@/lib/api";

const OS_TYPES = [
  ["windows", "Windows"],
  ["darwin", "macOS"],
  ["linux", "Linux"],
];
export function AntigravityFingerprintFields({
  value,
  onChange,
  proxies,
}: {
  value: Fingerprint;
  onChange: (value: Fingerprint) => void;
  proxies: ProxyOption[];
}) {
  const id = useId();
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <Field>
        <FieldLabel htmlFor={`${id}-os`}>操作系统</FieldLabel>
        <Select value={value.os_type} onValueChange={(os_type) => onChange({ ...value, os_type })}>
          <SelectTrigger id={`${id}-os`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            {OS_TYPES.map(([os, label]) => (
              <SelectItem key={os} value={os}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-arch`}>架构</FieldLabel>
        <Select value={value.arch} onValueChange={(arch) => onChange({ ...value, arch })}>
          <SelectTrigger id={`${id}-arch`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            <SelectItem value="amd64">amd64</SelectItem>
            <SelectItem value="arm64">arm64</SelectItem>
          </SelectContent>
        </Select>
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-os-version`}>系统版本</FieldLabel>
        <Input
          id={`${id}-os-version`}
          value={value.os_version}
          onChange={(e) => onChange({ ...value, os_version: e.target.value })}
        />
        <FieldDescription>留空保留当前版本</FieldDescription>
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
      <Field>
        <FieldLabel htmlFor={`${id}-timezone`}>时区</FieldLabel>
        <Input
          id={`${id}-timezone`}
          value={value.timezone}
          onChange={(e) => onChange({ ...value, timezone: e.target.value })}
        />
      </Field>
    </div>
  );
}
