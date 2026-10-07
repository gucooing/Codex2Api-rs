"use client";
import { useId } from "react";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Input } from "@/components/ui/input";
import type { Fingerprint, Proxy } from "@/lib/api";

export function GrokFingerprintFields({
  value,
  onChange,
  proxies,
}: {
  value: Fingerprint;
  onChange: (value: Fingerprint) => void;
  proxies: Proxy[];
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
            {["Windows", "Mac OS", "Ubuntu", "Debian", "Fedora", "Arch"].map((os) => (
              <SelectItem key={os} value={os}>
                {os}
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
            <SelectItem value="x86_64">x86_64</SelectItem>
            <SelectItem value="aarch64">aarch64</SelectItem>
          </SelectContent>
        </Select>
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
