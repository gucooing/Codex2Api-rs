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
  const [proxyDraft, setProxyDraft] = useState<string | null>();
  const [cacheDraft, setCacheMinutes] = useState<string>();
  useErrorToast(resource.error);
  useErrorToast(proxies.error);
  const busy = actions.isBusy("desktop-settings");
  const disabled = !resource.ready || !proxies.ready || busy;
  const proxy = proxyDraft === undefined ? resource.data?.proxy_id : proxyDraft;
  const proxyValue = resource.data ? (proxy ?? "default") : "";
  const cacheMinutes =
    cacheDraft ?? (resource.data ? String(resource.data.resource_cache_minutes) : "");
  const selectProxy = (value: string) => {
    if (disabled || (value !== "default" && !proxies.data?.items.some((item) => item.id === value)))
      return;
    setProxyDraft(value === "default" ? null : value);
  };
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "desktop-settings",
      async () => {
        if (disabled || !resource.data) throw new Error("请先加载设置和代理列表");
        const minutes = Number(cacheMinutes.trim());
        if (!Number.isInteger(minutes) || minutes < 1 || minutes > 1440)
          throw new Error("公开资源缓存时间须为 1 到 1440 分钟");
        if (proxy != null && !proxies.data?.items.some((item) => item.id === proxy))
          throw new Error("所选代理已不存在，请重新选择");
        await request("/settings/desktop", {
          method: "PUT",
          body: { proxy_id: proxy ?? null, resource_cache_minutes: minutes },
        });
        setProxyDraft(undefined);
        setCacheMinutes(undefined);
        resource.reload();
      },
      "Desktop 支持设置已保存",
    );
  return {
    fieldId,
    resource,
    proxies,
    busy,
    disabled,
    proxyValue,
    cacheMinutes,
    selectProxy,
    setCacheMinutes,
    handleSubmit,
  } as const;
}

type GatewayNumber =
  "default_rpm" | "default_concurrency" | "pool_top_k" | "sticky_ttl_seconds" | "pool_wait_ms";
type GatewayDraft = Partial<
  Omit<GatewaySettings, GatewayNumber | "ua_rules"> & Record<GatewayNumber | "ua_rules", string>
>;

export function useGateway() {
  const id = useId();
  const actions = useActions();
  const resource = useResource<GatewaySettings>("/settings/gateway");
  const [draft, setDraft] = useState<GatewayDraft>({});
  useErrorToast(resource.error);
  const busy = actions.isBusy("gateway-settings");
  const disabled = !resource.ready || busy;
  const update = <K extends keyof GatewayDraft>(key: K, value: GatewayDraft[K]) =>
    setDraft((previous) => ({ ...previous, [key]: value }));
  const numberText = (key: GatewayNumber) =>
    draft[key] ?? (resource.data?.[key] == null ? "" : String(resource.data[key]));
  const numberInput = (key: GatewayNumber) => ({
    id: `${id}-${key}`,
    value: numberText(key),
    disabled,
    onChange: (event: React.ChangeEvent<HTMLInputElement>) => update(key, event.target.value),
  });
  const mode = draft.scheduler_mode ?? resource.data?.scheduler_mode;
  const uaMode = draft.ua_mode ?? resource.data?.ua_mode;
  const rules = draft.ua_rules ?? resource.data?.ua_rules.join("\n") ?? "";
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "gateway-settings",
      async () => {
        if (!resource.ready || !resource.data) throw new Error("请先加载设置");
        const integer = (key: GatewayNumber, label: string, min: number, max: number) => {
          const text = numberText(key).trim();
          const value = Number(text);
          if (!text || !Number.isSafeInteger(value) || value < min || value > max)
            throw new Error(`${label}须为 ${min} 到 ${max} 的整数`);
          return value;
        };
        const body: GatewaySettings = {
          default_rpm: integer("default_rpm", "RPM", 0, 1_000_000),
          default_concurrency: integer("default_concurrency", "默认并发限制", 0, 1_000_000),
          pool_top_k: integer("pool_top_k", "候选账户数", 1, 100),
          sticky_ttl_seconds: integer("sticky_ttl_seconds", "会话有效期", 60, 604800),
          pool_wait_ms: integer("pool_wait_ms", "粘连等待时间", 1, 60000),
          scheduler_mode: mode ?? resource.data.scheduler_mode,
          sticky_weighted: draft.sticky_weighted ?? resource.data.sticky_weighted,
          sticky_escape: draft.sticky_escape ?? resource.data.sticky_escape,
          ua_mode: uaMode ?? resource.data.ua_mode,
          ua_rules: rules
            .split(/\r?\n/)
            .map((rule) => rule.trim())
            .filter(Boolean),
        };
        await request("/settings/gateway", { method: "PUT", body });
        setDraft({});
        resource.reload();
      },
      "网关设置已保存",
    );
  const form = {
    id,
    rpm: numberInput("default_rpm"),
    concurrency: numberInput("default_concurrency"),
    candidates: { ...numberInput("pool_top_k"), disabled: disabled || mode !== "advanced" },
    sessionTtl: numberInput("sticky_ttl_seconds"),
    wait: numberInput("pool_wait_ms"),
    mode: {
      value: mode ?? "",
      disabled,
      onValueChange: (value: string) => {
        if (value === "advanced" || value === "traditional") update("scheduler_mode", value);
      },
    },
    weighted: {
      checked: draft.sticky_weighted ?? resource.data?.sticky_weighted ?? false,
      disabled: disabled || mode !== "advanced",
      onCheckedChange: (checked: boolean) => update("sticky_weighted", checked),
    },
    escape: {
      checked: draft.sticky_escape ?? resource.data?.sticky_escape ?? false,
      disabled,
      onCheckedChange: (checked: boolean) => update("sticky_escape", checked),
    },
    uaMode: {
      value: uaMode ?? "",
      disabled,
      onValueChange: (value: string) => {
        if (value === "blacklist" || value === "whitelist") update("ua_mode", value);
      },
    },
    uaHint:
      uaMode === "blacklist"
        ? "拒绝匹配的客户端"
        : uaMode === "whitelist"
          ? "仅允许匹配的客户端"
          : "",
    rules: {
      value: rules,
      disabled,
      onChange: (event: React.ChangeEvent<HTMLTextAreaElement>) =>
        update("ua_rules", event.target.value),
    },
  };
  return { resource, form, busy, disabled, handleSubmit } as const;
}

export type GatewayForm = ReturnType<typeof useGateway>["form"];

export const fields = [
  {
    key: "api_url",
    label: "OAuth 客户端基础地址",
  },
  {
    key: "metered_api_url",
    label: "按量 API 基础地址",
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
  const [draft, setDraft] = useState<
    Partial<{
      old_username: string;
      old_password: string;
      new_username: string;
      new_password: string;
    }>
  >({});
  const form = {
    old_username: draft.old_username ?? username,
    old_password: draft.old_password ?? "",
    new_username: draft.new_username ?? username,
    new_password: draft.new_password ?? "",
  };
  const update = (key: keyof typeof form, next: string) =>
    setDraft((value) => ({ ...value, [key]: next }));
  const busy = actions.isBusy("settings-security");
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "settings-security",
      async () => {
        if (disabled) throw new Error("请先加载设置");
        await request("/settings/security", { method: "PUT", body: form });
        window.dispatchEvent(new Event("admin-session-expired"));
        router.replace("/");
      },
      "已保存",
    );
  return { fieldId, form, update, busy, handleSubmit } as const;
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
