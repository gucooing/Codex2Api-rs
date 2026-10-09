"use client";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import {
  query,
  request,
  type AccountUsage,
  type Consumer,
  type Device,
  type Json,
  type List,
  type RpmLimit,
  type Supplier,
  type SupplierTag,
} from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import { useListResource, usePageControls } from "@/lib/pagination";
import { usePlatformPrefix } from "@/lib/platform-scope";
import { usePreference, useSavedFilters, validPageSize } from "@/lib/preferences";
import { mobileRecordColumns, recordColumns } from "@/lib/records";
import type { Dispatch, SetStateAction } from "react";
import { useId, useState } from "react";

export const subscriptionLabel = (status: Consumer["subscription_status"]) =>
  ({ active: "订阅有效", expired: "订阅已到期", free: "免费层" })[status];

export const recordKinds = [
  ["realtime_call", "语音通话创建记录"],
  ["task", "任务与执行来源"],
  ["task_execution", "任务执行记录"],
  ["task_operation", "任务操作结果"],
  ["conversation", "会话"],
  ["task_turn", "任务轮次"],
  ["task_read", "任务操作"],
  ["connector_catalog", "连接器目录"],
  ["mcp_operation", "连接器调用"],
  ["plugin_operation", "插件操作"],
  ["automation_operation", "自动化操作"],
  ["family_notices", "家庭关联提示"],
] as const;

export function useClientRecords() {
  const fieldId = useId();
  const [kind, setKind] = useState("logs");
  const kinds = [
    ["logs", "活动日志"],
    ["cloud_environment", "任务关联的云环境"],
    ["subscription_operation", "订阅操作记录"],
    ...recordKinds,
    ["state", "客户端偏好与安装状态"],
  ];

  return { fieldId, kind, setKind, kinds } as const;
}

export type ResetCreditRecord = {
  id: string;
  status: "available" | "redeemed" | "pending" | "expired" | "not_applied";
  granted_at: string;
  redeemed_at: string | null;
  redeemed_by: string | null;
  windows_reset: number;
  note: string;
  available_at: string;
  expires_at: string | null;
  source: "card" | "admin_reset";
};

export function useConsumerResetCredits({ id }: { id: string }) {
  const platformPrefix = usePlatformPrefix();
  const tableColumns1 = useColumnVisibility(
    "components/consumers.tsx:1",
    [
      "类型",
      "发放时间",
      "启用时间",
      "到期时间",
      "状态",
      "使用时间",
      "使用方",
      "重置窗口数",
      "管理备注",
      "操作",
    ],
    ["类型", "状态", "操作"],
  );
  const fieldId = useId();
  const dialogFocus = useDialogFocus();
  const [grantOpen, setGrantOpen] = useState(false);
  const [startMode, setStartMode] = useState("now");
  const path = `${platformPrefix}/${encodeURIComponent(id)}/reset-credits`;
  const resource = useListResource<ResetCreditRecord, { available_count: number }>(path);
  const actions = useActions();
  const [quantity, setQuantity] = useState("1");
  const [note, setNote] = useState("");
  const [activateAt, setActivateAt] = useState("");
  const [durationDays, setDurationDays] = useState("30");
  const busy = actions.isBusy(`reset-credits-${id}`);
  const rows = resource.pagination;
  useErrorToast(resource.error);
  const handleClick = (credit: ResetCreditRecord) =>
    actions.run(
      `reset-credits-${id}`,
      async () => {
        const result = await request<{ code: string }>(`${path}/consume`, {
          method: "POST",
          body: {
            credit_id: credit.id,
          },
        });
        resource.reload();
        if (result.code === "nothing_to_reset")
          throw new Error("没有可重置的当前用量，或订阅已到期；未扣卡。");
        if (result.code === "no_credit") throw new Error("重置卡不可用，请刷新后重试。");
      },
      {
        confirm: "使用这张重置卡清零当前用量并重开额度周期？订阅到期时间及历史账单保持不变。",
        success: "重置已完成",
      },
    );
  return {
    tableColumns1,
    fieldId,
    dialogFocus,
    grantOpen,
    setGrantOpen,
    startMode,
    setStartMode,
    path,
    resource,
    actions,
    quantity,
    setQuantity,
    note,
    setNote,
    activateAt,
    setActivateAt,
    durationDays,
    setDurationDays,
    busy,
    rows,
    handleClick,
  } as const;
}

export function useConsumerUsage({ id }: { id: string }) {
  const platformPrefix = usePlatformPrefix();
  const resource = useResource<AccountUsage>(`${platformPrefix}/${id}/usage`);
  useErrorToast(resource.error);

  return { resource } as const;
}

export function useDevices({ id }: { id: string }) {
  const platformPrefix = usePlatformPrefix();
  const tableColumns2 = useColumnVisibility(
    "components/consumers.tsx:2",
    ["客户端 / 安装标识", "授权范围", "首次登录", "最近续期 / 使用", "操作"],
    ["客户端 / 安装标识", "最近续期 / 使用", "操作"],
  );
  const tableColumns3 = useColumnVisibility(
    "components/consumers.tsx:3:" + "remote_servers",
    recordColumns("remote_servers"),
    mobileRecordColumns("remote_servers"),
  );
  const actions = useActions();
  const resource = useListResource<Device>(`${platformPrefix}/${id}/devices`);
  const serverResource = useListResource<Json>(`${platformPrefix}/${id}/remote-servers`);
  const devices = resource.pagination;
  const servers = serverResource.pagination;
  useErrorToast(serverResource.error);
  useErrorToast(resource.error);
  const handleClick = (device: Device) =>
    void actions.run(
      "components\\consumers.tsx:action:17",
      async () => {
        await request(`${platformPrefix}/${id}/devices/${device.id}/revoke`, {
          method: "POST",
          body: {},
        });
        resource.reload();
      },
      {
        confirm: "撤销此设备授权并使其下线？",
        danger: true,
        success: undefined,
      },
    );
  return {
    tableColumns2,
    tableColumns3,
    actions,
    resource,
    serverResource,
    devices,
    servers,
    handleClick,
  } as const;
}

export type GrantResetCreditsDialogProps = {
  grantOpen: boolean;
  busy: boolean;
  setGrantOpen: Dispatch<SetStateAction<boolean>>;
  dialogFocus: ReturnType<typeof useDialogFocus>;
  actions: ReturnType<typeof useActions>;
  id: string;
  path: string;
  quantity: string;
  note: string;
  startMode: string;
  activateAt: string;
  durationDays: string;
  setNote: Dispatch<SetStateAction<string>>;
  resource: ReturnType<typeof useListResource<ResetCreditRecord, { available_count: number }>>;
  fieldId: string;
  setQuantity: Dispatch<SetStateAction<string>>;
  setStartMode: Dispatch<SetStateAction<string>>;
  setActivateAt: Dispatch<SetStateAction<string>>;
  setDurationDays: Dispatch<SetStateAction<string>>;
};

export function useGrantResetCreditsDialog({
  setGrantOpen,
  actions,
  id,
  path,
  quantity,
  note,
  startMode,
  activateAt,
  durationDays,
  setNote,
  resource,
}: Pick<
  GrantResetCreditsDialogProps,
  | "setGrantOpen"
  | "actions"
  | "id"
  | "path"
  | "quantity"
  | "note"
  | "startMode"
  | "activateAt"
  | "durationDays"
  | "setNote"
  | "resource"
>) {
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      `reset-credits-${id}`,
      async () => {
        await request(path, {
          method: "POST",
          body: {
            quantity: Number(quantity),
            note: note.trim(),
            activate_at: startMode === "scheduled" ? new Date(activateAt).toISOString() : null,
            duration_days: Number(durationDays),
          },
        });
        setNote("");
        setGrantOpen(false);
        resource.reload();
      },
      "重置卡已发放",
    );
  return { handleSubmit } as const;
}

export function useLogs({ id }: { id: string }) {
  const platformPrefix = usePlatformPrefix();
  const tableColumns5 = useColumnVisibility(
    "components/consumers.tsx:5:" + "logs",
    recordColumns("logs"),
    mobileRecordColumns("logs"),
  );
  const tableColumns6 = useColumnVisibility(
    "components/consumers.tsx:6:" + "analytics",
    recordColumns("analytics"),
    mobileRecordColumns("analytics"),
  );
  const tableColumns7 = useColumnVisibility(
    "components/consumers.tsx:7:" + "site_status",
    recordColumns("site_status"),
    mobileRecordColumns("site_status"),
  );
  const fieldId = useId();
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = usePreference<number>("logs.page-size", 20, validPageSize);
  const empty = { path: "", method: "", result: "" };
  const {
    filters,
    setFilters,
    applied,
    setApplied,
    ready: preferencesReady,
  } = useSavedFilters(`consumer:${id}:logs.filters`, empty);
  const resource = useResource<
    List<Json> & {
      total: number;
      page: number;
      page_size: number;
    }
  >(
    preferencesReady
      ? `${platformPrefix}/${id}/logs${query({ page, page_size: pageSize, ...applied })}`
      : null,
  );
  const rows = resource.data?.items ?? [];
  const pagination = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    setPage,
    pageSize,
    resource.refreshing,
    setPageSize,
  );
  const analyticsResource = useListResource<Json>(`${platformPrefix}/${id}/records`, {
    kind: "analytics",
  });
  const sitesResource = useListResource<Json>(`${platformPrefix}/${id}/records`, {
    kind: "site_status",
  });
  const analytics = analyticsResource.pagination;
  const sites = sitesResource.pagination;
  useErrorToast(analyticsResource.error);
  useErrorToast(sitesResource.error);
  useErrorToast(resource.error);

  return {
    tableColumns5,
    tableColumns6,
    tableColumns7,
    fieldId,
    setPage,
    empty,
    filters,
    setFilters,
    setApplied,
    resource,
    rows,
    pagination,
    analyticsResource,
    sitesResource,
    analytics,
    sites,
  } as const;
}

export function useRecords({ id, kind }: { id: string; kind: string }) {
  const platformPrefix = usePlatformPrefix();
  const tableColumns4 = useColumnVisibility(
    "components/consumers.tsx:4:" + kind,
    recordColumns(kind),
    mobileRecordColumns(kind),
  );
  const resource = useListResource<Json>(`${platformPrefix}/${id}/records`, { kind });
  const pagination = resource.pagination;
  useErrorToast(resource.error);

  return { tableColumns4, resource, pagination } as const;
}

export function useRouting({ id }: { id: string }) {
  const platformPrefix = usePlatformPrefix();
  const resource = useResource<RoutingResponse>(`${platformPrefix}/${id}/routing`);
  useErrorToast(resource.error);

  return { resource } as const;
}

export type RoutingResponse = {
  items: {
    virtual_account_id: string;
    provider_id: string;
    supplier_account_id: string | null;
    tag_id: string | null;
    revision: number;
  }[];
};

export function useRoutingForm({
  account,
  data,
  disabled,
  onSaved,
}: {
  account?: Consumer;
  data: RoutingResponse;
  disabled: boolean;
  onSaved: () => void;
}) {
  const platformPrefix = usePlatformPrefix();
  const id = useId();
  const actions = useActions();
  const rpm = useResource<RpmLimit>(account ? `${platformPrefix}/${account.id}/rate-limit` : null);
  const route = data.items.find((r) => r.provider_id === account?.provider_id);
  const [draft, setDraft] = useState<{ tag: string; supplier: string }>();
  const current = draft ?? { tag: route?.tag_id ?? "", supplier: route?.supplier_account_id ?? "" };
  const [rpmDraft, setRpm] = useState<string>();
  const rpmValue = rpmDraft ?? (rpm.data?.rpm == null ? "" : String(rpm.data.rpm));
  const [supplierSearch, setSupplierSearch] = useState("");
  const [supplierOpen, setSupplierOpen] = useState(false);
  const tags = useResource<List<SupplierTag>>(
    account ? `/supplier-tags/options${query({ provider_id: account.provider_id })}` : null,
  );
  const suppliers = useResource<List<Supplier>>(
    supplierOpen && current.tag
      ? `/suppliers/options${query({ provider_id: account?.provider_id, tag: current.tag, for_routing: true, search: supplierSearch, limit: 5 })}`
      : null,
    supplierSearch ? 250 : 0,
  );
  const assignedResource = useResource<Supplier>(
    current.supplier ? `/suppliers/${encodeURIComponent(current.supplier)}` : null,
  );
  const assigned = assignedResource.data ?? null;
  useErrorToast(assignedResource.error);
  useErrorToast(tags.error);
  useErrorToast(suppliers.error);
  useErrorToast(rpm.error);
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(event, "pool-route", async () => {
      if (disabled || !account || !tags.ready) throw new Error("请先加载号池和供应账户");
      await request(`${platformPrefix}/${account.id}/routing`, {
        method: "PUT",
        body: {
          tag_id: current.tag || null,
          supplier_id: current.supplier || null,
          revision: route?.revision ?? null,
        },
      });
      setDraft(undefined);
      onSaved();
      suppliers.reload();
    });
  const handleSubmit2 = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(event, "consumer-rpm", async () => {
      if (!account || !rpm.ready) throw new Error("请先加载 RPM 配置");
      const value = rpmValue.trim() === "" ? null : Number(rpmValue);
      if (value !== null && (!Number.isInteger(value) || value < 0 || value > 1_000_000))
        throw new Error("RPM 须为 0 到 1000000 的整数");
      await request(`${platformPrefix}/${account.id}/rate-limit`, {
        method: "PUT",
        body: { rpm: value },
      });
      setRpm(undefined);
      rpm.reload();
    });
  return {
    id,
    actions,
    rpm,
    setDraft,
    current,
    setRpm,
    rpmValue,
    setSupplierSearch,
    supplierOpen,
    setSupplierOpen,
    tags,
    suppliers,
    assigned,
    handleSubmit,
    handleSubmit2,
  } as const;
}
