"use client";
import { useIsMobile } from "@/hooks/use-mobile";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import {
  copyElementText,
  useActions,
  useDialogFocus,
  useErrorToast,
  validateForm,
} from "@/lib/actions";
import type { ProxyOption, SupplierTag } from "@/lib/api";
import { query, request, type Fingerprint, type List, type OAuth, type Supplier } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { mergeOAuth } from "@/lib/domain";
import { useQueryId, useResource } from "@/lib/hooks";
import { useListResource } from "@/lib/pagination";
import { usePreference, useSavedFilters, validView } from "@/lib/preferences";
import { supplierChannel } from "@/lib/providers";
import { parseRefreshTokenLines } from "@/lib/refresh-tokens";
import type { SupplierSelection } from "@/lib/supplier-selection";
import { commonSupplierTags, selectedSupplierProvider } from "@/lib/supplier-selection";
import { useId, useRef, useState } from "react";
import { toast } from "sonner";

export function useFingerprintEditor({
  account,
  onSaved,
  disabled,
}: {
  account?: Supplier;
  onSaved: () => void;
  disabled: boolean;
}) {
  const actions = useActions();
  const proxies = useResource<List<ProxyOption>>("/proxies/options");
  const [changes, setChanges] = useState<Partial<Fingerprint>>({});
  const value: Fingerprint = {
    ...(account?.fingerprint ?? {
      os_type: "",
      os_version: "",
      arch: "",
      terminal: "",
      proxy_id: null,
      timezone: "",
    }),
    ...changes,
  };
  useErrorToast(proxies.error);
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "components\\suppliers.tsx:form:12",
      async () => {
        if (disabled || !account) throw new Error("请先加载供应账户资料");
        await request(`/suppliers/${account.id}/fingerprint`, {
          method: "PUT",
          body: value,
        });
        onSaved();
      },
      "已保存",
    );
  return { actions, proxies, setChanges, value, handleSubmit } as const;
}

export function useOAuthWizard({
  supplierId,
  supplierProvider = "chatgpt",
  onClose,
  onComplete,
}: {
  supplierId?: string;
  supplierProvider?: string;
  onClose: () => void;
  onComplete: () => void;
}) {
  const dialogFocus = useDialogFocus();
  const fieldId = useId();
  const actions = useActions();
  const [provider, setProvider] = useState(supplierProvider);
  const channel = supplierChannel(provider);
  const setup = useResource<{ fingerprint: Fingerprint }>(
    supplierId ? null : `${channel.oauthPrefix}/setup`,
  );
  const proxies = useResource<List<ProxyOption>>(supplierId ? null : "/proxies/options");
  useErrorToast(setup.error);
  useErrorToast(proxies.error);
  const [step, setStep] = useState(supplierId ? 1 : 0);
  const [fingerprint, setFingerprint] = useState<Fingerprint>();
  const [method, setMethod] = useState<"callback" | "device" | "refresh_token">("callback");
  const [refreshToken, setRefreshToken] = useState("");
  const [batchResults, setBatchResults] = useState<
    {
      line: number;
      status: "pending" | "running" | "complete" | "failed" | "duplicate";
      message: string;
      supplierId?: string;
    }[]
  >([]);
  const [flow, setFlow] = useState<OAuth>();
  const [callback, setCallback] = useState("");
  const [pollAfter, setPollAfter] = useState(0);
  const pending = flow?.status === "pending" ? flow : undefined;
  const busy = actions.running.size > 0;
  const ready = Boolean(supplierId) || (setup.ready && proxies.ready);
  const methods = [
    { value: "callback", label: channel.callbackLabel, description: channel.callbackDescription },
    { value: "device", label: "设备码", description: "在设备授权页面输入一次性代码" },
    ...(!supplierId || channel.rtRelogin
      ? [{ value: "refresh_token", label: "RT 授权", description: "使用 Refresh Token 完成授权" }]
      : []),
  ];
  const steps = [
    ...(!supplierId ? [{ value: 0, label: "指纹配置" }] : []),
    { value: 1, label: "授权方式" },
    { value: 2, label: "开始授权" },
  ];
  const accept = (response: OAuth | { status: "pending" }) => {
    const value = mergeOAuth(flow, response);
    setFlow(value);
    if (value.status === "complete") {
      if (value.model_sync_error) toast.error(value.model_sync_error);
      onComplete();
    } else setPollAfter(Date.now() + (value.interval ?? 5) * 1000);
  };
  const cancelPending = async () => {
    if (pending) {
      await request(`${channel.oauthPrefix}/cancel`, {
        method: "POST",
        body: { state: pending.state },
      });
      setFlow(undefined);
      setCallback("");
    }
  };
  const close = () => {
    if (busy) return;
    void actions.run(
      "supplier-oauth-close",
      async () => {
        await cancelPending();
        if (batchResults.some((row) => row.status === "complete")) onComplete();
        else onClose();
      },
      { success: "" },
    );
  };
  const back = (previous: number) => {
    if (busy || previous >= step) return;
    void actions.run(
      "supplier-oauth-back",
      async () => {
        await cancelPending();
        setRefreshToken("");
        setStep(previous);
      },
      { success: "" },
    );
  };
  const start = async () => {
    if (!ready) throw new Error("请先加载授权资料");
    if (method === "refresh_token" && !supplierId) {
      const rows = parseRefreshTokenLines(refreshToken);
      const previous = new Map(batchResults.map((row) => [row.line, row]));
      setBatchResults(
        rows.map((row) =>
          previous.get(row.line)?.status === "complete"
            ? previous.get(row.line)!
            : {
                line: row.line,
                status: row.duplicateOf ? "duplicate" : "pending",
                message: row.duplicateOf ? `与第 ${row.duplicateOf} 行重复，已跳过` : "等待授权",
              },
        ),
      );
      let failures = 0;
      for (const row of rows) {
        if (row.duplicateOf || previous.get(row.line)?.status === "complete") continue;
        const updateResult = (
          status: "running" | "complete" | "failed",
          message: string,
          supplierId?: string,
        ) =>
          setBatchResults((old) =>
            old.map((item) =>
              item.line === row.line ? { ...item, status, message, supplierId } : item,
            ),
          );
        updateResult("running", "正在授权");
        try {
          const result = await request<OAuth>(`${channel.oauthPrefix}/start`, {
            method: "POST",
            body: {
              method,
              fingerprint: fingerprint ?? setup.data?.fingerprint,
              independent_fingerprint: rows.length > 1,
              refresh_token: row.token,
            },
          });
          if (result.status !== "complete") throw new Error("授权未完成");
          updateResult(
            "complete",
            result.model_sync_error
              ? "授权成功，模型目录待同步"
              : result.reused_existing
                ? "已更新原账户，保留原指纹"
                : "添加成功",
            result.supplier_id,
          );
        } catch {
          failures++;
          updateResult("failed", "授权失败，请检查 RT 或网络后重试");
        }
      }
      if (failures) toast.error(`${failures} 条 RT 授权失败，可检查后重试`);
      else toast.success("RT 导入完成");
      return;
    }
    accept(
      await request<OAuth>(
        supplierId ? `/suppliers/${supplierId}/relogin` : `${channel.oauthPrefix}/start`,
        {
          method: "POST",
          body: supplierId
            ? { method, refresh_token: method === "refresh_token" ? refreshToken : undefined }
            : {
                method,
                fingerprint: fingerprint ?? setup.data?.fingerprint,
                refresh_token: method === "refresh_token" ? refreshToken : undefined,
              },
        },
      ),
    );
    setRefreshToken("");
  };
  const authorize = async () => {
    if (!pending) return start();
    if (pending.method === "callback") {
      accept(
        await request<OAuth>(`${channel.oauthPrefix}/callback`, {
          method: "POST",
          body: channel.callbackBody(pending.state, callback),
        }),
      );
    } else {
      if (Date.now() < pollAfter) throw new Error("请完成官方授权后稍等片刻再检查。");
      accept(
        await request<OAuth>(`${channel.oauthPrefix}/poll`, {
          method: "POST",
          body: { state: pending.state },
        }),
      );
    }
  };
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (busy || !ready || !validateForm(event.currentTarget)) return;
    if (step === 0) {
      setStep(1);
      return;
    }
    if (step === 1) {
      setStep(2);
      if (method !== "refresh_token")
        void actions.run("supplier-oauth-start", start, { success: "" });
      return;
    }
    void actions.run("supplier-oauth-authorize", authorize, { success: "" });
  };
  const handleClick2 = () =>
    actions.run(
      "copy-supplier-authorize",
      async () => {
        await copyElementText(document.getElementById(`${fieldId}-authorize-url`));
      },
      { success: "授权地址已复制" },
    );
  return {
    dialogFocus,
    fieldId,
    actions,
    provider,
    setProvider,
    channel,
    setup,
    proxies,
    step,
    fingerprint,
    setFingerprint,
    method,
    setMethod,
    refreshToken,
    setRefreshToken,
    batchResults,
    setBatchResults,
    callback,
    setCallback,
    pending,
    busy,
    ready,
    methods,
    steps,
    close,
    back,
    handleSubmit,
    handleClick2,
  } as const;
}

export function useSupplierDetail() {
  const actions = useActions();
  const id = useQueryId();
  const resource = useResource<Supplier>(id ? `/suppliers/${encodeURIComponent(id)}` : null);
  const [tab, setTab] = useState("info");
  const [relogin, setRelogin] = useState(false);
  useErrorToast(id === "" ? "缺少供应账户编号。" : undefined);
  useErrorToast(resource.error ? resource.error : undefined);
  const account = resource.data;
  const profileRefresh = supplierChannel(account?.provider_id).profileRefresh;
  const handleClick = () =>
    void actions.run("supplier-profile", async () => {
      if (!profileRefresh) throw new Error("该平台不支持资料刷新");
      await request(profileRefresh(id), { method: "POST" });
      resource.reload();
    });
  const handleClick2 = () =>
    void actions.run(
      "components\\suppliers.tsx:action:5",
      async () => {
        await request(`/suppliers/${id}/status`, {
          method: "POST",
          body: { enabled: !account?.enabled },
        });
        resource.reload();
      },
      {
        confirm:
          account?.status === "active"
            ? "停用此供应账户？其虚拟账户将在下次请求时自动改选号池内可用账户。"
            : undefined,
        danger: false,
        success: undefined,
      },
    );
  return {
    actions,
    id,
    resource,
    tab,
    setTab,
    relogin,
    setRelogin,
    account,
    profileRefresh,
    handleClick,
    handleClick2,
  } as const;
}

export function useSupplierTagsBatchDialog() {
  const actions = useActions();
  const busy = actions.isBusy("supplier-tags-batch");

  return { busy } as const;
}

export const validSupplierFilters = (value: { status: string; provider_id: string }) =>
  ["", "active", "disabled", "error", "payment_required", "quota_exhausted"].includes(
    value.status,
  ) && ["", "chatgpt", "grok"].includes(value.provider_id);

export function useSuppliersPage() {
  const tableColumns0 = useColumnVisibility(
    "components/suppliers.tsx:0",
    ["供应账户", "提供商 / 订阅", "套餐到期", "状态", "额度", "标签", "绑定数", "最近使用", "操作"],
    ["供应账户", "状态", "操作"],
  );
  const mobile = useIsMobile();
  const fieldId = useId();
  const actions = useActions();
  const [add, setAdd] = useState(false);
  const [selected, setSelected] = useState<SupplierSelection[]>([]);
  const [batchOpen, setBatchOpen] = useState(false);
  const [batchAccounts, setBatchAccounts] = useState<SupplierSelection[]>([]);
  const empty = { search: "", provider_id: "", status: "", tag: "" };
  const { filters, setFilters, applied, setApplied } = useSavedFilters(
    "suppliers.filters",
    empty,
    validSupplierFilters,
  );
  const [view, setView] = usePreference<"table" | "cards">("suppliers.view", "table", validView);
  const now = useQuotaClock();
  const resource = useListResource<Supplier>("/suppliers", applied);
  const items = resource.data?.items ?? [];
  const pagination = resource.pagination;
  const selectedAccounts = selected;
  const selectedIds = new Set(selected.map((item) => item.id));
  const selectedProvider = selectedSupplierProvider(selectedAccounts);
  const tags = useResource<List<SupplierTag>>(
    `/supplier-tags/options${query({ provider_id: filters.provider_id })}`,
  );
  const tagOptions = tags.data?.items ?? [];
  useErrorToast(tags.error);
  const pageIds = pagination.rows.map((item) => item.id);
  const allPageSelected = pageIds.length > 0 && pageIds.every((id) => selectedIds.has(id));
  const somePageSelected = pageIds.some((id) => selectedIds.has(id));
  const selectionVersion = useRef(0);
  const clearSelection = () => {
    selectionVersion.current++;
    setSelected([]);
  };
  const batchBusy = actions.isBusy("supplier-tags-batch") || actions.isBusy("supplier-select");
  const toggleSelection = (ids: string[], checked: boolean) =>
    setSelected((current) => {
      const remaining = current.filter((item) => !ids.includes(item.id));
      return checked
        ? [
            ...remaining,
            ...items
              .filter((item) => ids.includes(item.id))
              .map(({ id, provider_id, tag_ids }) => ({ id, provider_id, tag_ids })),
          ]
        : remaining;
    });
  useErrorToast(resource.error);
  const handleSelect = (item: Supplier) =>
    void actions.run(
      "reset-state-" + item.id,
      async () => {
        await request(`/suppliers/${item.id}/reset-state`, { method: "POST" });
        resource.reload();
      },
      { success: "状态已重置" },
    );
  const handleSelect2 = (item: Supplier) =>
    void actions.run(
      "supplier-status-" + item.id,
      async () => {
        await request(`/suppliers/${item.id}/status`, {
          method: "POST",
          body: { enabled: !item.enabled },
        });
        resource.reload();
      },
      {
        confirm: item.enabled
          ? "停用此供应账户？其虚拟账户将在下次请求时自动改选号池内可用账户。"
          : undefined,
        danger: false,
        success: undefined,
      },
    );
  const handleSelect3 = (item: Supplier) =>
    void actions.run(
      "supplier-delete-" + item.id,
      async () => {
        await request(`/suppliers/${item.id}`, { method: "DELETE" });
        resource.reload();
      },
      {
        confirm: "删除供应账户及其上游授权？历史用量保留。",
        danger: true,
        success: "供应账户已删除",
      },
    );
  const handleClick4 = () =>
    void actions.run("supplier-select", async () => {
      const version = ++selectionVersion.current;
      const result = await request<List<SupplierSelection>>(
        `/suppliers/selection${query(applied)}`,
      );
      if (version === selectionVersion.current) setSelected(result.items);
    });
  return {
    tableColumns0,
    mobile,
    fieldId,
    actions,
    add,
    setAdd,
    batchOpen,
    setBatchOpen,
    batchAccounts,
    setBatchAccounts,
    empty,
    filters,
    setFilters,
    setApplied,
    view,
    setView,
    now,
    resource,
    items,
    pagination,
    selectedAccounts,
    selectedIds,
    selectedProvider,
    tags,
    tagOptions,
    pageIds,
    allPageSelected,
    somePageSelected,
    clearSelection,
    batchBusy,
    toggleSelection,
    handleSelect,
    handleSelect2,
    handleSelect3,
    handleClick4,
  } as const;
}

export function useSupplierTagEditor({
  accounts,
  disabled,
  batch = false,
  onSaved,
}: {
  accounts: SupplierSelection[];
  disabled: boolean;
  batch?: boolean;
  onSaved: () => void;
}) {
  const fieldId = useId();
  const [draft, setDraft] = useState<string[]>();
  const actions = useActions();
  const provider = selectedSupplierProvider(accounts);
  const tags = useResource<List<SupplierTag>>(
    provider ? `/supplier-tags/options${query({ provider_id: provider })}` : null,
  );
  const choices = tags.data?.items ?? [];
  const common = commonSupplierTags(accounts);
  const changed =
    draft !== undefined &&
    accounts.some(
      (account) =>
        account.tag_ids.length !== draft.length ||
        account.tag_ids.some((tag) => !draft.includes(tag)),
    );
  const key = batch ? "supplier-tags-batch" : "supplier-tags-single";
  const busy = actions.isBusy(key);
  const ready = !disabled && tags.ready && accounts.length > 0 && provider !== undefined;
  useErrorToast(tags.error);
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      key,
      async () => {
        if (!ready || draft === undefined || !changed)
          throw new Error("请加载账户资料并修改标签后再保存");
        await request("/suppliers/tags", {
          method: "POST",
          body: { account_ids: accounts.map((account) => account.id), tag_ids: draft },
        });
        setDraft(undefined);
        onSaved();
      },
      "标签已更新",
    );
  return {
    fieldId,
    draft,
    setDraft,
    provider,
    tags,
    choices,
    common,
    changed,
    busy,
    ready,
    handleSubmit,
  } as const;
}
