"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import type { ProxyOption } from "@/lib/api";
import {
  request,
  type DesktopSettings,
  type GatewaySettings,
  type List,
  type PublicUrlSettings,
} from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import { useListResource } from "@/lib/pagination";
import { useRouter } from "next/navigation";
import { useId, useState } from "react";

export function useDesktop() {
  const fieldId = useId();
  const actions = useActions();
  const resource = useResource<DesktopSettings>("/settings/desktop");
  const proxies = useResource<List<ProxyOption>>("/proxies/options");
  const [value, setValue] = useState<DesktopSettings>();
  useErrorToast(resource.error ? resource.error : undefined);
  useErrorToast(proxies.error);
  const current = value ?? resource.data ?? { proxy_id: null, resource_cache_minutes: 0 };
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "app\\settings\\page.tsx:form:9",
      async () => {
        if (!resource.ready) throw new Error("请先加载设置");
        await request("/settings/desktop", { method: "PUT", body: current });
      },
      "Desktop 支持设置已保存",
    );
  return { fieldId, actions, resource, proxies, setValue, current, handleSubmit } as const;
}

export function useGateway() {
  const fieldId = useId();
  const actions = useActions();
  const resource = useResource<GatewaySettings>("/settings/gateway");
  const [value, setValue] = useState<GatewaySettings>();
  const [rulesText, setRulesText] = useState<string>();
  const [rpmText, setRpmText] = useState<string>();
  useErrorToast(resource.error ? resource.error : undefined);
  const current = value ?? resource.data ?? { ua_mode: "blacklist", ua_rules: [], default_rpm: 20 };
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "app\\settings\\page.tsx:form:1",
      async () => {
        if (!resource.ready) throw new Error("请先加载设置");
        const text = rpmText ?? String(current.default_rpm);
        const defaultRpm = Number(text);
        if (
          !text.trim() ||
          !Number.isInteger(defaultRpm) ||
          defaultRpm < 0 ||
          defaultRpm > 1_000_000
        )
          throw new Error("RPM 须为 0 到 1000000 的整数");
        await request("/settings/gateway", {
          method: "PUT",
          body: {
            ...current,
            default_rpm: defaultRpm,
            ua_rules: (rulesText ?? current.ua_rules.join("\n"))
              .split(/\r?\n/)
              .map((item) => item.trim())
              .filter(Boolean),
          },
        });
      },
      "网关设置已保存",
    );
  return {
    fieldId,
    actions,
    resource,
    setValue,
    rulesText,
    setRulesText,
    rpmText,
    setRpmText,
    current,
    handleSubmit,
  } as const;
}

export const fields = [
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

export function usePublicUrls() {
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
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
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
    );
  return { fieldId, resource, setDraft, current, busy, disabled, handleSubmit } as const;
}

export function useResourceTable({ path, columns }: { path: string; columns: string[] }) {
  const tableColumns0 = useColumnVisibility(
    "app/settings/page.tsx:0:" + path,
    columns,
    columns.slice(0, 2),
  );
  const resource = useListResource<Record<string, unknown>>(path);
  useErrorToast(resource.error);
  const pagination = resource.pagination;
  const items = pagination.rows;

  return { tableColumns0, resource, pagination, items } as const;
}

export function useSecurityEditor({ username, disabled }: { username: string; disabled: boolean }) {
  const fieldId = useId();
  const actions = useActions();
  const router = useRouter();
  const [form, setForm] = useState({
    old_username: username,
    old_password: "",
    new_username: username,
    new_password: "",
  });
  const update = (key: keyof typeof form, next: string) =>
    setForm((value) => ({ ...value, [key]: next }));
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "app\\settings\\page.tsx:form:4",
      async () => {
        if (disabled) throw new Error("请先加载设置");
        await request("/settings/security", { method: "PUT", body: form });
        window.dispatchEvent(new Event("admin-session-expired"));
        router.replace("/");
      },
      "已保存",
    );
  return { fieldId, actions, form, update, handleSubmit } as const;
}

export function useSecurity() {
  const resource = useResource<{ username: string }>("/settings/security");
  useErrorToast(resource.error);

  return { resource } as const;
}

export function useSettingsPage() {
  const [tab, setTab] = useState("gateway");

  return { tab, setTab } as const;
}
