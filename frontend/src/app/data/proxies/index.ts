"use client";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import { request, type Proxy, type ProxyWrite } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useListResource } from "@/lib/pagination";
import { useSavedFilters } from "@/lib/preferences";
import { useId, useState } from "react";

export function useProxiesPage() {
  const tableColumns0 = useColumnVisibility(
    "app/proxies/page.tsx:0",
    ["代理", "绑定账户", "出口 / 时区", "连接检查", "质量检查", "操作"],
    ["代理", "连接检查", "操作"],
  );
  const fieldId = useId();
  const actions = useActions();
  const [editing, setEditing] = useState<Proxy | "new">();
  const empty = { search: "", protocol: "", result: "" };
  const { filters, setFilters, applied, setApplied } = useSavedFilters("proxies.filters", empty);
  const resource = useListResource<Proxy>("/proxies", applied);
  const pagination = resource.pagination;
  const check = async (proxy: Proxy, action: string) => {
    await request(`/proxies/${proxy.id}/check/${action}`, { method: "POST", body: {} });
    resource.reload();
  };
  useErrorToast(resource.error);
  const handleClick = (proxy: Proxy) =>
    void actions.run("app\\proxies\\page.tsx:action:4", () => check(proxy, "test"), {
      confirm: undefined,
      danger: false,
      success: undefined,
    });
  const handleSelect2 = (proxy: Proxy) =>
    void actions.run("app\\proxies\\page.tsx:action:4", () => check(proxy, "test"));
  const handleSelect3 = (proxy: Proxy) =>
    void actions.run("app\\proxies\\page.tsx:action:5", () => check(proxy, "quality"), {
      confirm: undefined,
      danger: false,
      success: undefined,
    });
  const handleSelect4 = (proxy: Proxy) =>
    void actions.run("app\\proxies\\page.tsx:action:6", () => check(proxy, "timezone"), {
      confirm: undefined,
      danger: false,
      success: undefined,
    });
  const handleSelect5 = (proxy: Proxy) =>
    void actions.run(
      "app\\proxies\\page.tsx:action:7",
      async () => {
        await request(`/proxies/${proxy.id}`, {
          method: "DELETE",
          body: { confirm_unbind: proxy.account_count > 0 },
        });
        resource.reload();
      },
      {
        confirm: proxy.account_count
          ? `此代理关联 ${proxy.account_count} 个供应账户，删除并解除绑定？`
          : "删除此代理？",
        danger: true,
        success: "代理已删除",
      },
    );
  return {
    tableColumns0,
    fieldId,
    actions,
    editing,
    setEditing,
    empty,
    filters,
    setFilters,
    setApplied,
    resource,
    pagination,
    handleClick,
    handleSelect2,
    handleSelect3,
    handleSelect4,
    handleSelect5,
  } as const;
}

export function useProxyEditor({ proxy, onSaved }: { proxy?: Proxy; onSaved: () => void }) {
  const dialogFocus = useDialogFocus();
  const fieldId = useId();
  const actions = useActions();
  const [value, setValue] = useState<ProxyWrite>({
    name: proxy?.name ?? "",
    protocol: proxy?.protocol ?? "http",
    host: proxy?.host ?? "",
    port: proxy?.port ?? 8080,
    username: proxy?.username ?? "",
    password: null,
  });
  const [passwordAction, setPasswordAction] = useState("keep");
  const update = <K extends keyof ProxyWrite>(key: K, next: ProxyWrite[K]) =>
    setValue((current) => ({ ...current, [key]: next }));
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "app\\proxies\\page.tsx:form:8",
      async () => {
        await request(proxy ? `/proxies/${proxy.id}` : "/proxies", {
          method: proxy ? "PUT" : "POST",
          body: {
            ...value,
            password:
              passwordAction === "keep" ? null : passwordAction === "remove" ? "" : value.password,
          },
        });
        onSaved();
      },
      "已保存",
    );
  return {
    dialogFocus,
    fieldId,
    actions,
    value,
    passwordAction,
    setPasswordAction,
    update,
    handleSubmit,
  } as const;
}
