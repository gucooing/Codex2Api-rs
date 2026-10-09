"use client";
import { useIsMobile } from "@/hooks/use-mobile";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import type { PlanOption, SupplierQuotaWindow } from "@/lib/api";
import { query, request, type Consumer, type ConsumerWrite, type List } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useQueryId, useResource } from "@/lib/hooks";
import { usePageControls } from "@/lib/pagination";
import { usePreference, useSavedFilters, validPageSize, validView } from "@/lib/preferences";
import type { Dispatch, SetStateAction } from "react";
import { useId, useRef, useState } from "react";
import { toast } from "sonner";

export type Selection = {
  ids: string[];
  all_matching: boolean;
  filters: { search: string; status: string; subscription: string };
  excluded_ids: string[];
};

export type Operation = "grant_reset" | "reset" | "delete";

export type ConsumerBatchDialogProps = {
  batchOpen: boolean;
  batchBusy: boolean;
  setBatchOpen: Dispatch<SetStateAction<boolean>>;
  dialogFocus: ReturnType<typeof useDialogFocus>;
  batchDialog: { operation: Operation; selection: Selection; count: number } | null;
  actions: ReturnType<typeof useActions>;
  runBatch: () => Promise<void>;
  resource: ReturnType<
    typeof useResource<
      List<Consumer & { quota: { windows: SupplierQuotaWindow[] } }> & {
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

export function useConsumerBatchDialog({
  actions,
  runBatch,
}: Pick<ConsumerBatchDialogProps, "actions" | "runBatch">) {
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(event, "consumer-batch", runBatch, "");
  return { handleSubmit } as const;
}

export function useConsumerDetail() {
  const platformPrefix = "/consumers";
  const id = useQueryId();
  const fieldId = useId();
  const resource = useResource<Consumer>(id ? `${platformPrefix}/${encodeURIComponent(id)}` : null);
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

export function useConsumersPage() {
  const tableColumns0 = useColumnVisibility(
    "components/consumers.tsx:0",
    ["选择", "虚拟账户", "提供商", "当前权益", "订阅到期", "登录状态", "额度", "操作"],
    ["选择", "虚拟账户", "登录状态", "操作"],
  );
  const mobile = useIsMobile();
  const dialogFocus = useDialogFocus();
  const fieldId = useId();
  const actions = useActions();
  const [create, setCreate] = useState(false);
  const empty = { search: "", status: "", subscription: "" };
  const {
    filters,
    setFilters,
    applied,
    setApplied,
    ready: preferencesReady,
  } = useSavedFilters("consumers.filters", empty);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = usePreference<number>("page-size:/consumers", 20, validPageSize);
  const resource = useResource<
    List<Consumer & { quota: { windows: SupplierQuotaWindow[] } }> & {
      total: number;
      page: number;
      page_size: number;
    }
  >(preferencesReady ? `/consumers${query({ page, page_size: pageSize, ...applied })}` : null);
  const [view, setView] = usePreference<"table" | "cards">("consumers.view", "table", validView);
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
  const batchBusy = actions.isBusy("consumer-batch");
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
  const openBatch = (operation: Operation, account?: Consumer) => {
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
      "/consumers/batch",
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

export function useConsumerForm({
  account,
  onSaved,
  editing = Boolean(account),
  disabled = false,
}: {
  account?: Consumer;
  editing?: boolean;
  disabled?: boolean;
  onSaved: () => void;
}) {
  const fieldId = useId();
  const actions = useActions();
  const [changes, setChanges] = useState<Partial<ConsumerWrite>>({});
  const value: ConsumerWrite = {
    username: account?.username ?? "",
    name: account?.name ?? "",
    email: account?.email ?? "",
    provider_id: account?.provider_id ?? "chatgpt",
    plan_id: account?.plan_id ?? "",
    subscription_expires_at: account?.subscription_expires_at ?? null,
    enabled: account?.enabled ?? !editing,
    password: "",
    revision: account?.revision ?? null,
    user_revision: account?.user_revision ?? null,
    ...changes,
  };
  const update = <K extends keyof ConsumerWrite>(key: K, next: ConsumerWrite[K]) =>
    setChanges((current) => ({ ...current, [key]: next }));
  const busy = actions.isBusy("consumer-account");
  const plans = useResource<List<PlanOption>>(
    `/plans/options${query({ provider_id: value.provider_id })}`,
  );
  const planOptions = plans.data?.items ?? [];
  useErrorToast(plans.error);
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "consumer-account",
      async () => {
        if (disabled || (editing && !account)) throw new Error("请先加载账户资料");
        if (!value.plan_id) throw new Error("请选择订阅套餐");
        await request(account ? `/consumers/${account.id}` : "/consumers", {
          method: editing ? "PUT" : "POST",
          body: value,
        });
        setChanges((current) => ({ ...current, password: "" }));
        onSaved();
      },
      "已保存",
    );
  return { fieldId, value, update, busy, plans, planOptions, handleSubmit } as const;
}
