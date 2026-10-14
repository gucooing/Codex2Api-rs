"use client";
import { useIsMobile } from "@/hooks/use-mobile";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import type { PlanOption, SupplierQuotaWindow } from "@/lib/api";
import { query, request, type SubscriptionAccount, type List } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useQueryId, useResource } from "@/lib/hooks";
import { usePageControls } from "@/lib/pagination";
import { usePreference, useSavedFilters, validPageSize, validView } from "@/lib/preferences";
import type { Dispatch, SetStateAction } from "react";
import { useId, useRef, useState } from "react";
import { useUserLookup, type UserOption } from "@/lib/user-lookup";
import { toast } from "sonner";

export type Selection = {
  ids: string[];
  all_matching: boolean;
  filters: { search: string; status: string; subscription: string; user_id: string };
  excluded_ids: string[];
};

export type Operation = "grant_reset" | "reset" | "delete";

export type SubscriptionBatchDialogProps = {
  batchOpen: boolean;
  batchBusy: boolean;
  setBatchOpen: Dispatch<SetStateAction<boolean>>;
  dialogFocus: ReturnType<typeof useDialogFocus>;
  batchDialog: { operation: Operation; selection: Selection; count: number } | null;
  actions: ReturnType<typeof useActions>;
  runBatch: () => Promise<void>;
  resource: ReturnType<
    typeof useResource<
      List<SubscriptionAccount & { quota: { windows: SupplierQuotaWindow[] } }> & {
        total: number;
        page: number;
        page_size: number;
      }
    >
  >;
  fieldId: string;
  grantQuantity: string;
  setGrantQuantity: Dispatch<SetStateAction<string>>;
  grantStartMode: string;
  setGrantStartMode: Dispatch<SetStateAction<string>>;
  grantActivateAt: string;
  setGrantActivateAt: Dispatch<SetStateAction<string>>;
  grantDuration: string;
  setGrantDuration: Dispatch<SetStateAction<string>>;
  grantNote: string;
  setGrantNote: Dispatch<SetStateAction<string>>;
};

export function useSubscriptionBatchDialog({
  actions,
  runBatch,
}: Pick<SubscriptionBatchDialogProps, "actions" | "runBatch">) {
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(event, "subscription-batch", runBatch, "");
  return { handleSubmit } as const;
}

export function useSubscriptionDetail() {
  const platformPrefix = "/subscriptions";
  const id = useQueryId();
  const fieldId = useId();
  const resource = useResource<SubscriptionAccount>(
    id ? `${platformPrefix}/${encodeURIComponent(id)}` : null,
  );
  const [tab, setTab] = useState("settings");
  const [group, setGroup] = useState("account");
  const [refreshVersion, setRefreshVersion] = useState(0);
  useErrorToast(!id ? "缺少账户编号。" : undefined);
  useErrorToast(resource.error);
  const account = resource.data;

  return {
    platformPrefix,
    id,
    fieldId,
    resource,
    tab,
    setTab,
    group,
    setGroup,
    refreshVersion,
    setRefreshVersion,
    account,
  } as const;
}

export function useSubscriptionsPage(initialUserId = "") {
  const tableColumns0 = useColumnVisibility(
    "components/subscriptions.tsx:0",
    ["选择", "用户", "提供商", "当前权益", "订阅到期", "登录状态", "额度", "操作"],
    ["选择", "用户", "登录状态", "操作"],
  );
  const mobile = useIsMobile();
  const dialogFocus = useDialogFocus();
  const fieldId = useId();
  const actions = useActions();
  const [create, setCreate] = useState(false);
  const empty = { search: "", status: "", subscription: "", user_id: initialUserId };
  const {
    filters,
    setFilters,
    applied,
    setApplied,
    ready: preferencesReady,
  } = useSavedFilters("subscriptions.filters", empty);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = usePreference<number>(
    "page-size:/subscriptions",
    20,
    validPageSize,
  );
  const resource = useResource<
    List<SubscriptionAccount & { quota: { windows: SupplierQuotaWindow[] } }> & {
      total: number;
      page: number;
      page_size: number;
    }
  >(preferencesReady ? `/subscriptions${query({ page, page_size: pageSize, ...applied })}` : null);
  const [view, setView] = usePreference<"table" | "cards">(
    "subscriptions.view",
    "table",
    validView,
  );
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [selectAllMatching, setSelectAllMatching] = useState(false);
  const [batchDialog, setBatchDialog] = useState<{
    operation: Operation;
    selection: Selection;
    count: number;
  } | null>(null);
  const [batchOpen, setBatchOpen] = useState(false);
  const [grantActivateAt, setGrantActivateAt] = useState("");
  const [grantStartMode, setGrantStartMode] = useState("now");
  const [grantDuration, setGrantDuration] = useState("30");
  const [grantQuantity, setGrantQuantity] = useState("1");
  const [grantNote, setGrantNote] = useState("");
  const attempt = useRef<{ signature: string; id: string } | null>(null);
  const batchBusy = actions.isBusy("subscription-batch");
  const items = resource.data?.items ?? [];
  const now = useQuotaClock();
  const pagination = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    setPage,
    pageSize,
    resource.refreshing,
    setPageSize,
  );
  const isSelected = (id: string) => (selectAllMatching ? !excluded.has(id) : selected.has(id));
  const pageIds = items.map((account) => account.id);
  const allPageSelected = pageIds.length > 0 && pageIds.every(isSelected);
  const somePageSelected = pageIds.some(isSelected);
  const selectionCount = selectAllMatching
    ? Math.max(0, (resource.data?.total ?? 0) - excluded.size)
    : selected.size;
  const clearSelection = () => {
    setSelected(new Set());
    setExcluded(new Set());
    setSelectAllMatching(false);
  };
  const toggleIds = (ids: string[], checked: boolean) => {
    if (selectAllMatching) {
      const next = new Set(excluded);
      ids.forEach((id) => {
        if (checked) next.delete(id);
        else next.add(id);
      });
      setExcluded(next);
    } else {
      const next = new Set(selected);
      ids.forEach((id) => {
        if (checked) next.add(id);
        else next.delete(id);
      });
      setSelected(next);
    }
  };
  const togglePage = (checked: boolean | "indeterminate") => toggleIds(pageIds, checked === true);
  const openBatch = (operation: Operation, account?: SubscriptionAccount) => {
    if (!resource.ready || batchBusy) return;
    setBatchOpen(true);
    setGrantActivateAt("");
    setGrantStartMode("now");
    setGrantDuration("30");
    setGrantQuantity("1");
    setGrantNote("");
    setBatchDialog({
      operation,
      count: account ? 1 : selectionCount,
      selection: account
        ? { ids: [account.id], all_matching: false, filters: empty, excluded_ids: [] }
        : {
            ids: selectAllMatching ? [] : [...selected].sort(),
            all_matching: selectAllMatching,
            filters: { ...applied },
            excluded_ids: [...excluded].sort(),
          },
    });
  };
  const runBatch = async () => {
    if (!batchDialog || !batchOpen || !resource.ready) return;
    const body = {
      operation: batchDialog.operation,
      ...batchDialog.selection,
      ...(batchDialog.operation === "grant_reset"
        ? {
            quantity: Number(grantQuantity),
            note: grantNote.trim(),
            activate_at:
              grantStartMode === "scheduled" ? new Date(grantActivateAt).toISOString() : null,
            duration_days: Number(grantDuration),
          }
        : {}),
    };
    const signature = JSON.stringify(body);
    if (attempt.current?.signature !== signature)
      attempt.current = { signature, id: crypto.randomUUID() };
    const result = await request<{ matched: number; affected: number; skipped: number }>(
      "/subscriptions/batch",
      {
        method: "POST",
        body: { ...body, request_id: attempt.current.id },
      },
    );
    attempt.current = null;
    clearSelection();
    setBatchOpen(false);
    resource.reload();
    const verb =
      body.operation === "delete" ? "删除" : body.operation === "reset" ? "重置" : "发卡";
    toast.success(
      `${verb}完成：${result.affected} 个账户${result.skipped ? `；跳过 ${result.skipped} 个账户` : ""}`,
    );
  };
  useErrorToast(resource.error);

  return {
    tableColumns0,
    mobile,
    dialogFocus,
    fieldId,
    actions,
    create,
    setCreate,
    empty,
    filters,
    setFilters,
    setApplied,
    setPage,
    resource,
    view,
    setView,
    setExcluded,
    selectAllMatching,
    setSelectAllMatching,
    batchDialog,
    batchOpen,
    setBatchOpen,
    grantActivateAt,
    setGrantActivateAt,
    grantStartMode,
    setGrantStartMode,
    grantDuration,
    setGrantDuration,
    grantQuantity,
    setGrantQuantity,
    grantNote,
    setGrantNote,
    batchBusy,
    items,
    now,
    pagination,
    isSelected,
    pageIds,
    allPageSelected,
    somePageSelected,
    selectionCount,
    clearSelection,
    toggleIds,
    togglePage,
    openBatch,
    runBatch,
  } as const;
}

type SubscriptionDraft = {
  user_id: string;
  provider_id: string;
  plan_id: string;
  subscription_expires_at: string | null;
  enabled: boolean;
  reissue: boolean;
};
export function useSubscriptionForm({
  account,
  actions: sharedActions,
  onSaved,
  editing = Boolean(account),
  disabled = false,
}: {
  account?: SubscriptionAccount;
  actions?: ReturnType<typeof useActions>;
  onSaved: () => void;
  editing?: boolean;
  disabled?: boolean;
}) {
  const fieldId = useId();
  const localActions = useActions();
  const popupContainer = useRef<HTMLFormElement>(null);
  const actions = sharedActions ?? localActions;
  const [changes, setChanges] = useState<Partial<SubscriptionDraft>>({});
  const [chosenUser, setChosenUser] = useState<UserOption | null>(null);
  const users = useUserLookup();
  const value: SubscriptionDraft = {
    user_id: account?.user_id ?? "",
    provider_id: account?.provider_id ?? "chatgpt",
    plan_id: account?.plan_id ?? "",
    subscription_expires_at: account?.subscription_expires_at ?? null,
    enabled: account?.enabled ?? true,
    reissue: false,
    ...changes,
  };
  const update = <K extends keyof SubscriptionDraft>(key: K, next: SubscriptionDraft[K]) =>
    setChanges((current) => ({ ...current, [key]: next }));
  const plans = useResource<List<PlanOption>>(
    `/plans/options${query({ provider_id: value.provider_id })}`,
  );
  const busy = actions.isBusy("subscription-account");
  const planOptions = plans.data?.items ?? [];
  const selectProvider = (provider: string) => {
    if (editing || provider === value.provider_id || !["chatgpt", "grok"].includes(provider))
      return;
    setChanges((current) => ({ ...current, provider_id: provider, plan_id: "" }));
  };
  const selectPlan = (plan: string) => {
    if (planOptions.some((item) => item.id === plan)) update("plan_id", plan);
  };
  useErrorToast(plans.error);
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(event, "subscription-account", async () => {
      if (disabled || !plans.ready || (editing && !account)) throw new Error("请先加载订阅资料");
      if (!value.user_id || !planOptions.some((plan) => plan.id === value.plan_id))
        throw new Error("请选择用户和同平台套餐");
      const expiry = value.subscription_expires_at ? new Date(value.subscription_expires_at) : null;
      if (expiry && !Number.isFinite(expiry.getTime())) throw new Error("请填写有效到期时间");
      await request(account ? `/subscriptions/${account.id}` : "/subscriptions", {
        method: editing ? "PUT" : "POST",
        body: {
          user_id: value.user_id,
          plan_id: value.plan_id,
          expires_at: expiry?.toISOString() ?? null,
          enabled: value.enabled,
          reissue: value.reissue,
          revision: account?.revision ?? null,
        },
      });
      setChanges({});
      onSaved();
    });
  return {
    popupContainer,
    fieldId,
    value,
    update,
    selectProvider,
    selectPlan,
    busy,
    plans,
    planOptions,
    handleSubmit,
    users,
    chosenUser,
    setChosenUser,
  } as const;
}
