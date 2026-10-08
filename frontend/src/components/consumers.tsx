"use client";
import type { PlanOption } from "@/lib/api";
import { PlatformApiContext, usePlatformPrefix } from "@/lib/platform-scope";
import { RoutingForm, type RoutingResponse } from "@/components/consumer-routing";
import { useColumnVisibility } from "@/lib/columns";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useListResource, usePageControls } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight, Columns3 } from "lucide-react";

import { useDialogFocus } from "@/lib/actions";
import { ScrollArea } from "@/components/ui/scroll-area";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuCheckboxItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import { MoreHorizontal, Table2, LayoutGrid, Inbox, X } from "lucide-react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  CardAction,
} from "@/components/ui/card";
import {
  Field,
  FieldLabel,
  FieldDescription,
  FieldGroup,
  FieldTitle,
  FieldSet,
  FieldContent,
  FieldSeparator,
} from "@/components/ui/field";
import { useId } from "react";

import { Input } from "@/components/ui/input";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableBody,
  TableCell,
} from "@/components/ui/table";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogClose,
  DialogTrigger,
  DialogDescription,
} from "@/components/ui/dialog";
import { useActions, useErrorToast } from "@/lib/actions";
import { Switch } from "@/components/ui/switch";
import { Progress } from "@/components/ui/progress";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import { quotaWindowLabel, quotaResetLabel, percentLabel } from "@/lib/supplier-state";
import type { SupplierQuotaWindow } from "@/lib/api";
import { Checkbox } from "@/components/ui/checkbox";
import { toast } from "sonner";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { date } from "@/lib/format";
import Link from "next/link";
import { useRef, useState } from "react";
import { Plus, Search, RotateCcw, RefreshCw, ArrowRight } from "lucide-react";
import {
  request,
  query,
  type Consumer,
  type ConsumerWrite,
  type List,
  type Device,
  type Json,
} from "@/lib/api";
import { localDate, scopes } from "@/lib/domain";
import { ResourceRefreshContext, useQueryId, useResource } from "@/lib/hooks";

import { ConsumerUsage } from "./usage";
import { ConfigPanel, ClientStatePanel } from "./config";
import { supplierChannel } from "@/components/providers";
import { recordColumns, recordRows, mobileRecordColumns } from "@/lib/records";
import { usePreference, useSavedFilters, validView, validPageSize } from "@/lib/preferences";
import { useIsMobile } from "@/hooks/use-mobile";

const subscriptionLabel = (status: Consumer["subscription_status"]) =>
  ({ active: "订阅有效", expired: "订阅已到期", free: "免费层" })[status];
export function ConsumersPage() {
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
  type Selection = {
    ids: string[];
    all_matching: boolean;
    filters: typeof empty;
    excluded_ids: string[];
  };
  type Operation = "grant_reset" | "reset" | "delete";
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
  const accountActions = (account: Consumer) => (
    <div className="flex items-center gap-2">
      <Button variant="outline" size="sm" asChild>
        <Link href={`/consumers/detail/?id=${encodeURIComponent(account.id)}`}>
          管理 <ArrowRight />
        </Link>
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button type="button" variant="ghost" size="icon-sm" aria-label="更多操作">
            <MoreHorizontal />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem
            disabled={!resource.ready || batchBusy}
            onSelect={() => openBatch("reset", account)}
          >
            重置用量
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!resource.ready || batchBusy}
            onSelect={() => openBatch("grant_reset", account)}
          >
            发放重置卡
          </DropdownMenuItem>
          <DropdownMenuItem
            variant="destructive"
            disabled={!resource.ready || batchBusy}
            onSelect={() => openBatch("delete", account)}
          >
            删除账户
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
  useErrorToast(resource.error);
  return (
    <>
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters });
              clearSelection();
              setPage(1);
              resource.reload();
            }}
          >
            <Field className="w-40">
              <FieldLabel
                htmlFor={fieldId + "-field-2" + "-" + encodeURIComponent(String("搜索账户"))}
              >
                {"搜索账户"}
              </FieldLabel>
              <Input
                id={fieldId + "-field-2" + "-" + encodeURIComponent(String("搜索账户"))}
                aria-label={"搜索账户"}
                value={filters.search}
                onChange={(e) => setFilters({ ...filters, search: e.target.value })}
                placeholder="名称、用户名或邮箱"
              />
            </Field>
            <Field className="w-40">
              <FieldLabel
                htmlFor={fieldId + "-field-3" + "-" + encodeURIComponent(String("登录状态"))}
              >
                {"登录状态"}
              </FieldLabel>
              <Select
                value={filters.status}
                onValueChange={(next) =>
                  ((status) => setFilters({ ...filters, status }))(
                    next ===
                      fieldId + "-field-3" + "-" + encodeURIComponent(String("登录状态")) + "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-3" + "-" + encodeURIComponent(String("登录状态"))}
                  aria-label={"登录状态"}
                  data-required={false ? "true" : undefined}
                  data-empty={String(filters.status) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部状态" },
                        { value: "enabled", label: "已启用" },
                        { value: "disabled", label: "已停用" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部状态" },
                    { value: "enabled", label: "已启用" },
                    { value: "disabled", label: "已停用" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-3" +
                          "-" +
                          encodeURIComponent(String("登录状态")) +
                          "-empty"
                      }
                      disabled={"disabled" in option && Boolean(option.disabled)}
                    >
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field className="w-40">
              <FieldLabel
                htmlFor={fieldId + "-field-4" + "-" + encodeURIComponent(String("订阅状态"))}
              >
                {"订阅状态"}
              </FieldLabel>
              <Select
                value={filters.subscription}
                onValueChange={(next) =>
                  ((subscription) => setFilters({ ...filters, subscription }))(
                    next ===
                      fieldId + "-field-4" + "-" + encodeURIComponent(String("订阅状态")) + "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-4" + "-" + encodeURIComponent(String("订阅状态"))}
                  aria-label={"订阅状态"}
                  data-required={false ? "true" : undefined}
                  data-empty={String(filters.subscription) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部订阅" },
                        { value: "active", label: "订阅有效" },
                        { value: "expired", label: "订阅已到期" },
                        { value: "free", label: "免费层" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部订阅" },
                    { value: "active", label: "订阅有效" },
                    { value: "expired", label: "订阅已到期" },
                    { value: "free", label: "免费层" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-4" +
                          "-" +
                          encodeURIComponent(String("订阅状态")) +
                          "-empty"
                      }
                      disabled={"disabled" in option && Boolean(option.disabled)}
                    >
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <div className="flex flex-wrap items-center gap-2 self-end">
              <Button type="submit">
                <Search />
                查询
              </Button>
              <Button
                variant="secondary"
                type="button"
                onClick={() => {
                  setFilters(empty);
                  setApplied(empty);
                  clearSelection();
                  setPage(1);
                  resource.reload();
                }}
              >
                <RotateCcw />
                重置
              </Button>
            </div>
          </form>
          <div className="flex flex-wrap items-center gap-2 self-end xl:ml-auto">
            <div className="flex items-center gap-2">
              <Checkbox
                aria-label="选择当前页账户"
                checked={allPageSelected ? true : somePageSelected ? "indeterminate" : false}
                disabled={!resource.ready || batchBusy}
                onCheckedChange={togglePage}
              />
              <span className="text-sm text-muted-foreground">选择账户</span>
            </div>
            {selectionCount > 0 && (
              <>
                <Badge variant="secondary">
                  已选择 {selectionCount} 个{selectAllMatching ? "（全部筛选结果）" : ""}
                </Badge>
                <Button
                  type="button"
                  variant="link"
                  size="sm"
                  disabled={batchBusy}
                  onClick={clearSelection}
                >
                  取消选择
                </Button>
                {!selectAllMatching && (resource.data?.total ?? 0) > pageIds.length && (
                  <Button
                    size="sm"
                    variant="link"
                    type="button"
                    disabled={!resource.ready || batchBusy}
                    onClick={() => {
                      setSelectAllMatching(true);
                      setExcluded(new Set());
                    }}
                  >
                    选择当前筛选的全部 {resource.data?.total ?? "—"} 个
                  </Button>
                )}
                <Button
                  size="sm"
                  variant="outline"
                  type="button"
                  disabled={!resource.ready || batchBusy || selectionCount === 0}
                  onClick={() => openBatch("reset")}
                >
                  重置
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  type="button"
                  disabled={!resource.ready || batchBusy || selectionCount === 0}
                  onClick={() => openBatch("grant_reset")}
                >
                  发放重置卡
                </Button>
                <Button
                  size="sm"
                  variant="destructive"
                  type="button"
                  disabled={!resource.ready || batchBusy || selectionCount === 0}
                  onClick={() => openBatch("delete")}
                >
                  删除
                </Button>
              </>
            )}
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={resource.reload}
              aria-label="刷新账户"
            >
              <RefreshCw />
              刷新
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button type="button" variant="outline" size="sm" aria-label="显示列">
                  <Columns3 />
                  显示列
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-48">
                <DropdownMenuLabel>
                  {tableColumns0.mobile ? "手机显示列" : "桌面显示列"}
                </DropdownMenuLabel>
                {tableColumns0.labels.map((label) => (
                  <DropdownMenuCheckboxItem
                    key={label}
                    checked={tableColumns0.isVisible(label)}
                    disabled={tableColumns0.count === 1 && tableColumns0.isVisible(label)}
                    onSelect={(event) => event.preventDefault()}
                    onCheckedChange={(checked) => tableColumns0.setVisible(label, checked === true)}
                  >
                    {label}
                  </DropdownMenuCheckboxItem>
                ))}
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={tableColumns0.showAll}>显示全部列</DropdownMenuItem>
                <DropdownMenuItem onSelect={tableColumns0.reset}>恢复默认列</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
            <ToggleGroup
              className="hidden md:flex"
              type="single"
              variant="outline"
              size="sm"
              value={view}
              onValueChange={(next) => {
                if (next === "table" || next === "cards") setView(next);
              }}
              aria-label="视图切换"
            >
              <ToggleGroupItem value="table" aria-label="表格视图">
                <Table2 />
                表格
              </ToggleGroupItem>
              <ToggleGroupItem value="cards" aria-label="卡片视图">
                <LayoutGrid />
                卡片
              </ToggleGroupItem>
            </ToggleGroup>
            {
              <Button type="button" onClick={() => setCreate(true)}>
                <Plus />
                创建虚拟账户
              </Button>
            }
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent className="space-y-4">
          {mobile || view === "table" ? (
            <Table
              className={
                tableColumns0.count > 4
                  ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
              }
              role="table"
            >
              <TableHeader>
                <TableRow role="row">
                  {[
                    "选择",
                    "虚拟账户",
                    "提供商",
                    "当前权益",
                    "订阅到期",
                    "登录状态",
                    "额度",
                    "操作",
                  ].map((label) => (
                    <TableHead
                      hidden={!tableColumns0.isVisible(label)}
                      className={
                        ["选择", "虚拟账户", "登录状态", "操作"].includes(label)
                          ? label === "操作"
                            ? "max-md:w-28"
                            : label === "选择"
                              ? "max-md:w-7"
                              : label === "虚拟账户"
                                ? ""
                                : "max-md:w-16"
                          : ""
                      }
                      key={label}
                      scope="col"
                    >
                      {label}
                    </TableHead>
                  ))}
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.length ? (
                  <>
                    {items.map((account) => (
                      <TableRow role="row" key={account.id}>
                        <TableCell
                          hidden={!tableColumns0.isVisible("选择")}
                          className=" max-md:overflow-hidden"
                          data-label="选择"
                          role="cell"
                        >
                          <Checkbox
                            aria-label={`选择 ${account.name}`}
                            checked={isSelected(account.id)}
                            disabled={!resource.ready || batchBusy}
                            onCheckedChange={(checked) => toggleIds([account.id], checked === true)}
                          />
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("虚拟账户")}
                          className=" max-md:overflow-hidden"
                          data-label="虚拟账户"
                          role="cell"
                        >
                          <div className="max-md:hidden">
                            <Link
                              className="block max-w-56 truncate"
                              title={account.name}
                              href={`/consumers/detail/?id=${encodeURIComponent(account.id)}`}
                            >
                              <strong>{account.name}</strong>
                            </Link>
                            <CardDescription
                              className="max-w-56 truncate"
                              title={`${account.username} · ${account.email}`}
                            >
                              {account.username} · {account.email}
                            </CardDescription>
                          </div>
                          <Dialog>
                            <DialogTrigger asChild>
                              <Button
                                type="button"
                                variant="ghost"
                                className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                                aria-label={"查看详情：" + String(account.name)}
                              >
                                <span className="min-w-0 flex-1">
                                  <span className="block truncate font-medium">{account.name}</span>
                                  <span className="block truncate text-xs text-muted-foreground">
                                    {account.username}
                                  </span>
                                </span>
                                <ChevronRight className="size-3 shrink-0" />
                              </Button>
                            </DialogTrigger>
                            <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
                              <DialogHeader>
                                <DialogTitle>记录详情</DialogTitle>
                                <DialogDescription>当前记录的完整字段</DialogDescription>
                              </DialogHeader>
                              <FieldGroup className="gap-3">
                                <Field>
                                  <FieldTitle>虚拟账户</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    <Link
                                      className="block max-w-56 truncate"
                                      title={account.name}
                                      href={`/consumers/detail/?id=${encodeURIComponent(account.id)}`}
                                    >
                                      <strong>{account.name}</strong>
                                    </Link>
                                    <CardDescription
                                      className="max-w-56 truncate"
                                      title={`${account.username} · ${account.email}`}
                                    >
                                      {account.username} · {account.email}
                                    </CardDescription>
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>提供商</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    {account.provider_id}
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>当前权益</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    {account.plan_name}
                                    <CardDescription>
                                      {subscriptionLabel(account.subscription_status)}
                                    </CardDescription>
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>订阅到期</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    {account.subscription_expires_at
                                      ? date(account.subscription_expires_at)
                                      : "未设置到期时间"}
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>登录状态</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    <Badge variant={account.enabled ? "secondary" : "outline"}>
                                      {account.enabled ? "已启用" : "已停用"}
                                    </Badge>
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>额度</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    <div
                                      className="flex w-64 flex-wrap gap-2"
                                      aria-label="账户额度"
                                    >
                                      {account.quota.windows.map((window) => (
                                        <div
                                          key={window.id}
                                          className="min-w-0 flex-1 basis-28 space-y-1 text-xs"
                                        >
                                          <div
                                            className="truncate"
                                            title={
                                              window.reset_at == null
                                                ? "首次使用后计时"
                                                : quotaResetLabel(window.reset_at, now)
                                            }
                                          >
                                            {quotaWindowLabel(window)}：
                                            {window.reset_at == null
                                              ? "首次使用后计时"
                                              : quotaResetLabel(window.reset_at, now)}
                                          </div>
                                          <div className="flex items-center gap-2">
                                            {window.used_percent != null ? (
                                              <>
                                                <Progress
                                                  value={Math.min(100, window.used_percent)}
                                                  className="h-1 flex-1 [&>[data-slot=progress-indicator]]:bg-foreground"
                                                  aria-label={quotaWindowLabel(window) + "额度已用"}
                                                />
                                                <span className="shrink-0 whitespace-nowrap tabular-nums">
                                                  {percentLabel(window.used_percent)}
                                                </span>
                                              </>
                                            ) : (
                                              <span className="text-muted-foreground">不限额</span>
                                            )}
                                          </div>
                                        </div>
                                      ))}
                                      {account.quota.windows.length === 0 && (
                                        <span className="text-xs text-muted-foreground">
                                          不限额
                                        </span>
                                      )}
                                    </div>
                                  </div>
                                </Field>
                              </FieldGroup>
                            </DialogContent>
                          </Dialog>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("提供商")}
                          className=" "
                          data-label="提供商"
                          data-compact="true"
                          role="cell"
                        >
                          {account.provider_id}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("当前权益")}
                          className=" "
                          data-label="当前权益"
                          role="cell"
                        >
                          {account.plan_name}
                          <CardDescription>
                            {subscriptionLabel(account.subscription_status)}
                          </CardDescription>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("订阅到期")}
                          className=" "
                          data-label="订阅到期"
                          role="cell"
                        >
                          {account.subscription_expires_at
                            ? date(account.subscription_expires_at)
                            : "未设置到期时间"}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("登录状态")}
                          className=" max-md:overflow-hidden"
                          data-label="登录状态"
                          data-compact="true"
                          role="cell"
                        >
                          <Badge variant={account.enabled ? "secondary" : "outline"}>
                            {account.enabled ? "已启用" : "已停用"}
                          </Badge>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("额度")}
                          className=" "
                          data-label="额度"
                          role="cell"
                        >
                          <div className="flex w-64 flex-wrap gap-2" aria-label="账户额度">
                            {account.quota.windows.map((window) => (
                              <div
                                key={window.id}
                                className="min-w-0 flex-1 basis-28 space-y-1 text-xs"
                              >
                                <div
                                  className="truncate"
                                  title={
                                    window.reset_at == null
                                      ? "首次使用后计时"
                                      : quotaResetLabel(window.reset_at, now)
                                  }
                                >
                                  {quotaWindowLabel(window)}：
                                  {window.reset_at == null
                                    ? "首次使用后计时"
                                    : quotaResetLabel(window.reset_at, now)}
                                </div>
                                <div className="flex items-center gap-2">
                                  {window.used_percent != null ? (
                                    <>
                                      <Progress
                                        value={Math.min(100, window.used_percent)}
                                        className="h-1 flex-1 [&>[data-slot=progress-indicator]]:bg-foreground"
                                        aria-label={quotaWindowLabel(window) + "额度已用"}
                                      />
                                      <span className="shrink-0 whitespace-nowrap tabular-nums">
                                        {percentLabel(window.used_percent)}
                                      </span>
                                    </>
                                  ) : (
                                    <span className="text-muted-foreground">不限额</span>
                                  )}
                                </div>
                              </div>
                            ))}
                            {account.quota.windows.length === 0 && (
                              <span className="text-xs text-muted-foreground">不限额</span>
                            )}
                          </div>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("操作")}
                          className=" max-md:[&_button]:h-7 max-md:[&_button]:px-1.5 max-md:[&_button]:text-xs max-md:[&_button]:gap-1 max-md:[&_a]:h-7 max-md:[&_a]:px-1.5 max-md:[&_a]:text-xs max-md:[&_a]:gap-1 max-md:[&>div]:gap-1"
                          data-label="操作"
                          role="cell"
                        >
                          {accountActions(account)}
                        </TableCell>
                      </TableRow>
                    ))}
                  </>
                ) : (
                  <TableRow role="row">
                    <TableCell role="cell" colSpan={tableColumns0.count}>
                      <Empty>
                        <EmptyDescription>{"暂无符合条件的虚拟账户"}</EmptyDescription>
                      </Empty>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          ) : items.length ? (
            <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
              {items.map((account) => (
                <Card key={account.id}>
                  <CardContent className="space-y-4">
                    <Checkbox
                      aria-label={`选择 ${account.name}`}
                      checked={isSelected(account.id)}
                      disabled={!resource.ready || batchBusy}
                      onCheckedChange={(checked) => toggleIds([account.id], checked === true)}
                    />
                    <div className="flex items-start justify-between gap-2">
                      <div>
                        <Link href={`/consumers/detail/?id=${encodeURIComponent(account.id)}`}>
                          <strong>{account.name}</strong>
                        </Link>
                        <CardDescription>{account.username}</CardDescription>
                      </div>
                      <Badge variant={account.enabled ? "secondary" : "outline"}>
                        {account.enabled ? "已启用" : "已停用"}
                      </Badge>
                    </div>
                    <div className="space-y-3">
                      <CardDescription className="text-sm text-muted-foreground">
                        {account.email}
                      </CardDescription>
                      <FieldGroup className="grid gap-4 sm:grid-cols-2 gap-3">
                        {[
                          { label: "提供商", value: account.provider_id },
                          { label: "订阅套餐", value: account.plan_name },
                          {
                            label: "当前权益",
                            value: subscriptionLabel(account.subscription_status),
                          },
                          {
                            label: "订阅到期",
                            value: account.subscription_expires_at
                              ? date(account.subscription_expires_at)
                              : "未设置到期时间",
                          },
                        ].map(({ label, value }) => (
                          <Field key={label}>
                            <FieldTitle>{label}</FieldTitle>
                            <FieldDescription>{value ?? "—"}</FieldDescription>
                          </Field>
                        ))}
                      </FieldGroup>
                    </div>
                    <div className="flex w-64 flex-wrap gap-2" aria-label="账户额度">
                      {account.quota.windows.map((window) => (
                        <div key={window.id} className="min-w-0 flex-1 basis-28 space-y-1 text-xs">
                          <div
                            className="truncate"
                            title={
                              window.reset_at == null
                                ? "首次使用后计时"
                                : quotaResetLabel(window.reset_at, now)
                            }
                          >
                            {quotaWindowLabel(window)}：
                            {window.reset_at == null
                              ? "首次使用后计时"
                              : quotaResetLabel(window.reset_at, now)}
                          </div>
                          <div className="flex items-center gap-2">
                            {window.used_percent != null ? (
                              <>
                                <Progress
                                  value={Math.min(100, window.used_percent)}
                                  className="h-1 flex-1 [&>[data-slot=progress-indicator]]:bg-foreground"
                                  aria-label={quotaWindowLabel(window) + "额度已用"}
                                />
                                <span className="shrink-0 whitespace-nowrap tabular-nums">
                                  {percentLabel(window.used_percent)}
                                </span>
                              </>
                            ) : (
                              <span className="text-muted-foreground">不限额</span>
                            )}
                          </div>
                        </div>
                      ))}
                      {account.quota.windows.length === 0 && (
                        <span className="text-xs text-muted-foreground">不限额</span>
                      )}
                    </div>
                    <div className="border-t pt-3">{accountActions(account)}</div>
                  </CardContent>
                </Card>
              ))}
            </div>
          ) : (
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <Inbox />
                </EmptyMedia>
                <EmptyDescription>暂无符合条件的虚拟账户</EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
        </CardContent>
      </Card>
      <Pagination aria-label="账户分页" className="mt-3 justify-end">
        <PaginationContent className="flex-wrap justify-end gap-1">
          <PaginationItem>
            <Select {...pagination.size}>
              <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper" side="bottom" align="end">
                {[10, 20, 30, 50].map((size) => (
                  <SelectItem key={size} value={String(size)}>
                    {size} 条/页
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </PaginationItem>
          <PaginationItem className="mr-2 text-xs text-muted-foreground">
            共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
          </PaginationItem>
          <PaginationItem>
            <Button
              type="button"
              variant="outline"
              size="icon-sm"
              aria-label="首页"
              {...pagination.first}
            >
              <ChevronsLeft />
            </Button>
          </PaginationItem>
          <PaginationItem>
            <Button
              type="button"
              variant="outline"
              size="icon-sm"
              aria-label="上一页"
              {...pagination.previous}
            >
              <ChevronLeft />
            </Button>
          </PaginationItem>
          <PaginationItem>
            <Input className="h-7 w-14 text-center tabular-nums" {...pagination.input} />
          </PaginationItem>
          <PaginationItem>
            <Button
              type="button"
              variant="outline"
              size="icon-sm"
              aria-label="下一页"
              {...pagination.next}
            >
              <ChevronRight />
            </Button>
          </PaginationItem>
          <PaginationItem>
            <Button
              type="button"
              variant="outline"
              size="icon-sm"
              aria-label="末页"
              {...pagination.last}
            >
              <ChevronsRight />
            </Button>
          </PaginationItem>
        </PaginationContent>
      </Pagination>
      <Dialog
        open={batchOpen}
        onOpenChange={(open) => {
          if (!open && !batchBusy) setBatchOpen(false);
        }}
      >
        <DialogContent
          {...dialogFocus}
          showCloseButton={false}
          aria-describedby={undefined}
          onEscapeKeyDown={(event) => {
            if (batchBusy) event.preventDefault();
          }}
          onInteractOutside={(event) => {
            if (batchBusy) event.preventDefault();
          }}
        >
          <DialogHeader>
            <DialogTitle>
              {batchDialog?.operation === "grant_reset"
                ? "发放重置卡"
                : batchDialog?.operation === "reset"
                  ? "重置用量"
                  : "删除虚拟账户"}
            </DialogTitle>
          </DialogHeader>
          <form
            noValidate
            onSubmit={(event) => actions.submit(event, "consumer-batch", runBatch, "")}
          >
            <FieldSet disabled={batchBusy || !resource.ready || !batchOpen} className="gap-3">
              <CardDescription>
                已选择 {batchDialog?.count ?? 0} 个账户
                {batchDialog?.selection.all_matching ? "（全部筛选结果）" : ""}
              </CardDescription>
              {batchDialog?.operation === "grant_reset" ? (
                <>
                  <Field>
                    <FieldLabel htmlFor={`${fieldId}-batch-quantity`}>
                      发放数量（每个账户）
                    </FieldLabel>
                    <Input
                      id={`${fieldId}-batch-quantity`}
                      type="number"
                      min={1}
                      max={100}
                      step={1}
                      required
                      value={grantQuantity}
                      onChange={(e) => setGrantQuantity(e.target.value)}
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${fieldId}-batch-mode`}>启用方式</FieldLabel>
                    <Select
                      value={grantStartMode}
                      onValueChange={setGrantStartMode}
                      disabled={batchBusy}
                    >
                      <SelectTrigger id={`${fieldId}-batch-mode`}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        <SelectItem value="now">立即启用</SelectItem>
                        <SelectItem value="scheduled">定时启用</SelectItem>
                      </SelectContent>
                    </Select>
                  </Field>
                  {grantStartMode === "scheduled" && (
                    <Field>
                      <FieldLabel htmlFor={`${fieldId}-batch-start`}>启用时间</FieldLabel>
                      <Input
                        id={`${fieldId}-batch-start`}
                        type="datetime-local"
                        required
                        value={grantActivateAt}
                        onChange={(e) => setGrantActivateAt(e.target.value)}
                      />
                    </Field>
                  )}
                  <Field>
                    <FieldLabel htmlFor={`${fieldId}-batch-duration`}>有效时长（天）</FieldLabel>
                    <Input
                      id={`${fieldId}-batch-duration`}
                      type="number"
                      min={1}
                      max={3650}
                      step={1}
                      required
                      value={grantDuration}
                      onChange={(e) => setGrantDuration(e.target.value)}
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${fieldId}-batch-note`}>管理备注</FieldLabel>
                    <Input
                      id={`${fieldId}-batch-note`}
                      maxLength={256}
                      value={grantNote}
                      onChange={(e) => setGrantNote(e.target.value)}
                    />
                  </Field>
                </>
              ) : (
                <CardDescription>
                  {batchDialog?.operation === "delete"
                    ? "删除所选账户并撤销登录？"
                    : "重置所选账户用量及周期？订阅到期时间不变。"}
                </CardDescription>
              )}
              <div className="flex justify-end gap-2">
                <Button type="button" variant="outline" onClick={() => setBatchOpen(false)}>
                  取消
                </Button>
                <Button
                  type="submit"
                  variant={batchDialog?.operation === "delete" ? "destructive" : "default"}
                  disabled={!batchDialog || batchDialog.count === 0 || !resource.ready || batchBusy}
                >
                  {batchBusy && <Spinner />}确认
                </Button>
              </div>
            </FieldSet>
          </form>
        </DialogContent>
      </Dialog>
      {create && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !actions.running.size) (() => setCreate(false))();
          }}
        >
          <DialogContent
            {...dialogFocus}
            showCloseButton={false}
            className="flex max-h-[90dvh] min-h-0 flex-col sm:max-w-3xl"
            aria-describedby={undefined}
            onEscapeKeyDown={(event) => {
              if (actions.running.size) event.preventDefault();
            }}
            onInteractOutside={(event) => {
              if (actions.running.size) event.preventDefault();
            }}
          >
            <DialogHeader>
              <DialogTitle>{"创建虚拟账户"}</DialogTitle>
            </DialogHeader>
            <DialogClose asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                className="absolute right-4 top-4"
                aria-label="关闭"
                disabled={actions.running.size > 0}
              >
                <X />
              </Button>
            </DialogClose>
            <ConsumerForm
              onCancel={() => setCreate(false)}
              onSaved={() => {
                setCreate(false);
                resource.reload();
              }}
            />
          </DialogContent>
        </Dialog>
      )}
    </>
  );
}
export function ConsumerForm({
  account,
  onSaved,
  onCancel,
  editing = Boolean(account),
  disabled = false,
}: {
  account?: Consumer;
  editing?: boolean;
  disabled?: boolean;
  onSaved: () => void;
  onCancel?: () => void;
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
  const fields = (
    <FieldSet disabled={disabled || busy} className="gap-3">
      <div
        className={
          onCancel ? "grid gap-3 sm:grid-cols-2" : "grid gap-3 sm:grid-cols-2 2xl:grid-cols-3"
        }
      >
        <Field>
          <FieldLabel htmlFor={`${fieldId}-name`}>账户名称</FieldLabel>
          <Input
            id={`${fieldId}-name`}
            required
            maxLength={128}
            value={value.name}
            onChange={(event) => update("name", event.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-username`}>登录用户名</FieldLabel>
          <Input
            id={`${fieldId}-username`}
            required
            maxLength={128}
            autoComplete="off"
            value={value.username}
            onChange={(event) => update("username", event.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-email`}>邮箱</FieldLabel>
          <Input
            id={`${fieldId}-email`}
            type="email"
            required
            maxLength={254}
            value={value.email}
            onChange={(event) => update("email", event.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-provider`}>提供商</FieldLabel>
          <Select
            value={value.provider_id}
            onValueChange={(provider) => update("provider_id", provider)}
            disabled={editing || disabled || busy}
          >
            <SelectTrigger id={`${fieldId}-provider`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="chatgpt">ChatGPT</SelectItem>
              <SelectItem value="grok">Grok</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-password`}>{editing ? "新密码" : "登录密码"}</FieldLabel>
          <Input
            id={`${fieldId}-password`}
            type="password"
            autoComplete="new-password"
            required={!editing}
            placeholder={editing ? "留空保留现有密码" : undefined}
            value={value.password}
            onChange={(event) => update("password", event.target.value)}
          />
        </Field>
        <Field orientation="horizontal" className="self-end pb-1">
          <Switch
            id={`${fieldId}-enabled`}
            checked={value.enabled}
            onCheckedChange={(enabled) => update("enabled", enabled)}
          />
          <FieldContent>
            <FieldLabel htmlFor={`${fieldId}-enabled`}>允许账户登录</FieldLabel>
          </FieldContent>
        </Field>
      </div>
      <FieldSeparator />
      <div className="grid gap-3 sm:grid-cols-2">
        <Field>
          <FieldLabel htmlFor={`${fieldId}-plan`}>订阅套餐</FieldLabel>
          <Select
            value={value.plan_id}
            onValueChange={(plan) => {
              if (!plan || disabled || busy || !plans.ready) return;
              update("plan_id", plan === `${fieldId}-empty-plan` ? "" : plan);
            }}
            disabled={disabled || busy || !plans.ready}
          >
            <SelectTrigger
              id={`${fieldId}-plan`}
              className="w-full"
              data-required="true"
              data-empty={!value.plan_id ? "true" : undefined}
            >
              <SelectValue placeholder={account?.plan_name || "请选择套餐"} />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value={`${fieldId}-empty-plan`}>
                {plans.loading ? "正在加载套餐…" : "请选择套餐"}
              </SelectItem>
              {planOptions.map((plan) => (
                <SelectItem key={plan.id} value={plan.id}>
                  {plan.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-expires`}>订阅到期时间</FieldLabel>
          <Input
            id={`${fieldId}-expires`}
            type="datetime-local"
            value={localDate(value.subscription_expires_at)}
            onChange={(event) =>
              update(
                "subscription_expires_at",
                event.target.value ? new Date(event.target.value).toISOString() : null,
              )
            }
          />
        </Field>
      </div>
    </FieldSet>
  );
  return (
    <form
      noValidate
      className="flex min-h-0 flex-col gap-3"
      aria-busy={busy}
      onSubmit={(event) =>
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
        )
      }
    >
      {onCancel ? (
        <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
          {fields}
        </ScrollArea>
      ) : (
        fields
      )}
      <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
        {onCancel && (
          <Button type="button" variant="outline" disabled={disabled || busy} onClick={onCancel}>
            取消
          </Button>
        )}
        <Button type="submit" disabled={disabled || busy || !plans.ready}>
          {busy && <Spinner />}
          {busy ? "正在提交…" : editing ? "保存账户与订阅" : "创建账户"}
        </Button>
      </FieldGroup>
    </form>
  );
}
export function ConsumerDetail({ regular = false }: { regular?: boolean }) {
  return (
    <PlatformApiContext value={regular ? "/subscriptions" : "/consumers"}>
      <PlatformDetail />
    </PlatformApiContext>
  );
}
function PlatformDetail() {
  const platformPrefix = usePlatformPrefix();
  const id = useQueryId();
  const fieldId = useId();
  const resource = useResource<Consumer>(id ? `${platformPrefix}/${encodeURIComponent(id)}` : null);
  const [tab, setTab] = useState("settings");
  const [group, setGroup] = useState("account");
  const [refreshVersion, setRefreshVersion] = useState(0);
  useErrorToast(!id ? "缺少账户编号。" : undefined);
  useErrorToast(resource.error);
  if (!id)
    return (
      <Empty>
        <EmptyDescription>
          {platformPrefix === "/subscriptions" ? "请选择用户订阅" : "请选择虚拟账户"}
        </EmptyDescription>
        <Button asChild variant="outline">
          <Link href={platformPrefix + "/"}>返回列表</Link>
        </Button>
      </Empty>
    );
  const account = resource.data;
  return (
    <ResourceRefreshContext value={refreshVersion}>
      <Tabs value={tab} onValueChange={setTab} className="min-w-0 gap-3">
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2">
          <div className="min-w-0 max-w-full flex-1 overflow-x-auto overflow-y-hidden pb-1">
            <TabsList
              variant="line"
              aria-label={platformPrefix === "/subscriptions" ? "用户订阅详情" : "虚拟账户详情"}
            >
              {[
                ["settings", "账户设置"],
                ["usage", "用量统计"],
                ["reset-credits", "重置卡"],
                ["records", "记录查询"],
                ["devices", "登录设备"],
              ].map(([key, label]) => (
                <TabsTrigger key={key} value={key}>
                  {label}
                </TabsTrigger>
              ))}
            </TabsList>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                resource.reload();
                setRefreshVersion((value) => value + 1);
              }}
              disabled={resource.refreshing}
            >
              {resource.refreshing && <Spinner />}刷新
            </Button>
            <Button variant="outline" size="sm" asChild>
              <Link
                href={
                  account?.user_kind === "regular" && account.user_id
                    ? `/subscriptions/?user_id=${encodeURIComponent(account.user_id)}`
                    : "/consumers/"
                }
              >
                返回列表
              </Link>
            </Button>
          </div>
        </div>
        <TabsContent value={tab} className="space-y-3">
          {tab === "settings" && (
            <>
              <Field orientation="horizontal" className="w-fit flex-wrap">
                <FieldLabel htmlFor={`${fieldId}-group`}>设置分类</FieldLabel>
                <Select value={group} onValueChange={setGroup}>
                  <SelectTrigger id={`${fieldId}-group`} className="w-48">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    {supplierChannel(account?.provider_id).configGroups.map((item) => (
                      <SelectItem key={item.key} value={item.key}>
                        {item.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              {group === "account" ? (
                <div className="grid items-start gap-3 xl:grid-cols-[minmax(0,2fr)_minmax(18rem,1fr)]">
                  <Card size="sm">
                    <CardHeader>
                      <CardTitle role="heading" aria-level={2}>
                        账户与订阅
                      </CardTitle>
                      <CardAction className="flex flex-wrap justify-end gap-2">
                        <Badge variant="secondary">
                          {account ? subscriptionLabel(account.subscription_status) : "加载中"}
                        </Badge>
                        <Badge variant="outline">
                          {account ? (account.enabled ? "登录已启用" : "登录已停用") : "加载中"}
                        </Badge>
                      </CardAction>
                      <CardDescription>创建于 {date(account?.created_at)}</CardDescription>
                    </CardHeader>
                    <CardContent>
                      {platformPrefix === "/consumers" ? (
                        <ConsumerForm
                          key={id}
                          editing
                          disabled={!resource.ready}
                          account={account}
                          onSaved={resource.reload}
                        />
                      ) : (
                        <div className="space-y-3">
                          <dl className="grid gap-3 text-sm sm:grid-cols-2">
                            <div>
                              <dt className="text-muted-foreground">用户名</dt>
                              <dd>{account?.username ?? "—"}</dd>
                            </div>
                            <div>
                              <dt className="text-muted-foreground">名称</dt>
                              <dd>{account?.name ?? "—"}</dd>
                            </div>
                            <div>
                              <dt className="text-muted-foreground">邮箱</dt>
                              <dd>{account?.email ?? "—"}</dd>
                            </div>
                            <div>
                              <dt className="text-muted-foreground">订阅套餐</dt>
                              <dd>{account?.plan_name ?? "—"}</dd>
                            </div>
                          </dl>
                          <Button asChild variant="outline">
                            <Link
                              href={
                                account?.user_id
                                  ? `/subscriptions/?user_id=${encodeURIComponent(account.user_id)}`
                                  : "/subscriptions/"
                              }
                            >
                              管理用户订阅
                            </Link>
                          </Button>
                        </div>
                      )}
                    </CardContent>
                  </Card>
                  <Routing key={id} id={id} account={account} />
                </div>
              ) : (
                <ConfigPanel key={id} id={id} group={group} />
              )}
            </>
          )}
          {tab === "usage" && <ConsumerUsage key={id} id={id} />}
          {tab === "reset-credits" && <ConsumerResetCredits key={id} id={id} />}
          {tab === "records" && <ClientRecords key={id} id={id} />}
          {tab === "devices" && <Devices key={id} id={id} />}
        </TabsContent>
      </Tabs>
    </ResourceRefreshContext>
  );
}
type ResetCreditRecord = {
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
function ConsumerResetCredits({ id }: { id: string }) {
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
  return (
    <>
      <div className="flex items-center gap-3">
        <Button
          size="sm"
          disabled={!resource.ready || busy}
          onClick={() => {
            setStartMode("now");
            setActivateAt("");
            setDurationDays("30");
            setQuantity("1");
            setNote("");
            setGrantOpen(true);
          }}
        >
          发放重置卡
        </Button>
        <Badge variant="secondary">可用 {resource.data?.available_count ?? "—"} 张</Badge>
      </div>
      <Dialog
        open={grantOpen}
        onOpenChange={(open) => {
          if (!busy) setGrantOpen(open);
        }}
      >
        <DialogContent
          {...dialogFocus}
          showCloseButton={false}
          aria-describedby={undefined}
          onEscapeKeyDown={(e) => {
            if (busy) e.preventDefault();
          }}
          onInteractOutside={(e) => {
            if (busy) e.preventDefault();
          }}
        >
          <DialogHeader>
            <DialogTitle>发放重置卡</DialogTitle>
          </DialogHeader>
          <form
            noValidate
            className="grid gap-3"
            onSubmit={(event) =>
              actions.submit(
                event,
                `reset-credits-${id}`,
                async () => {
                  await request(path, {
                    method: "POST",
                    body: {
                      quantity: Number(quantity),
                      note: note.trim(),
                      activate_at:
                        startMode === "scheduled" ? new Date(activateAt).toISOString() : null,
                      duration_days: Number(durationDays),
                    },
                  });
                  setNote("");
                  setGrantOpen(false);
                  resource.reload();
                },
                "重置卡已发放",
              )
            }
          >
            <Field>
              <FieldLabel htmlFor={`${fieldId}-quantity`}>发放数量</FieldLabel>
              <Input
                id={`${fieldId}-quantity`}
                type="number"
                min={1}
                max={100}
                step={1}
                required
                value={quantity}
                onChange={(event) => setQuantity(event.target.value)}
                disabled={!resource.ready || busy}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor={`${fieldId}-note`}>管理备注</FieldLabel>
              <Input
                id={`${fieldId}-note`}
                maxLength={256}
                value={note}
                onChange={(event) => setNote(event.target.value)}
                disabled={!resource.ready || busy}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor={`${fieldId}-start-mode`}>启用方式</FieldLabel>
              <Select
                value={startMode}
                onValueChange={setStartMode}
                disabled={!resource.ready || busy}
              >
                <SelectTrigger id={`${fieldId}-start-mode`}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="now">立即启用</SelectItem>
                  <SelectItem value="scheduled">定时启用</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            {startMode === "scheduled" && (
              <Field>
                <FieldLabel htmlFor={`${fieldId}-activate`}>启用时间</FieldLabel>
                <Input
                  id={`${fieldId}-activate`}
                  type="datetime-local"
                  required
                  value={activateAt}
                  onChange={(event) => setActivateAt(event.target.value)}
                  disabled={!resource.ready || busy}
                />
              </Field>
            )}
            <Field>
              <FieldLabel htmlFor={`${fieldId}-duration`}>有效时长（天）</FieldLabel>
              <Input
                id={`${fieldId}-duration`}
                type="number"
                min={1}
                max={3650}
                step={1}
                required
                value={durationDays}
                onChange={(e) => setDurationDays(e.target.value)}
                disabled={!resource.ready || busy}
              />
            </Field>
            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                disabled={busy}
                onClick={() => setGrantOpen(false)}
              >
                取消
              </Button>
              <Button type="submit" disabled={!resource.ready || busy}>
                {busy && <Spinner />}确认发放
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
      <>
        <div className="mb-2 flex justify-end">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" variant="outline" size="sm" aria-label="显示列">
                <Columns3 />
                显示列
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <DropdownMenuLabel>
                {tableColumns1.mobile ? "手机显示列" : "桌面显示列"}
              </DropdownMenuLabel>
              {tableColumns1.labels.map((label) => (
                <DropdownMenuCheckboxItem
                  key={label}
                  checked={tableColumns1.isVisible(label)}
                  disabled={tableColumns1.count === 1 && tableColumns1.isVisible(label)}
                  onSelect={(event) => event.preventDefault()}
                  onCheckedChange={(checked) => tableColumns1.setVisible(label, checked === true)}
                >
                  {label}
                </DropdownMenuCheckboxItem>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={tableColumns1.showAll}>显示全部列</DropdownMenuItem>
              <DropdownMenuItem onSelect={tableColumns1.reset}>恢复默认列</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        <Table
          className={
            tableColumns1.count > 4
              ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
              : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
          }
          role="table"
        >
          <TableHeader>
            <TableRow role="row">
              <TableHead hidden={!tableColumns1.isVisible("类型")} className="">
                类型
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("发放时间")} className="">
                发放时间
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("启用时间")} className="">
                启用时间
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("到期时间")} className="">
                到期时间
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("状态")} className="max-md:w-20">
                状态
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("使用时间")} className="">
                使用时间
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("使用方")} className="">
                使用方
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("重置窗口数")} className="">
                重置窗口数
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("管理备注")} className="">
                管理备注
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("操作")} className="max-md:w-20">
                操作
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.rows.map((credit) => (
              <TableRow role="row" key={credit.id}>
                <TableCell
                  hidden={!tableColumns1.isVisible("类型")}
                  className=" max-md:overflow-hidden"
                  data-label="类型"
                  data-compact="true"
                  role="cell"
                >
                  <div className="max-md:hidden">
                    {credit.source === "admin_reset" ? "管理员直接重置" : "重置卡"}
                  </div>
                  <Dialog>
                    <DialogTrigger asChild>
                      <Button
                        type="button"
                        variant="ghost"
                        className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                        aria-label={
                          "查看详情：" +
                          String(credit.source === "admin_reset" ? "管理员直接重置" : "重置卡")
                        }
                      >
                        <span className="min-w-0 flex-1">
                          <span className="block truncate font-medium">
                            {credit.source === "admin_reset" ? "管理员直接重置" : "重置卡"}
                          </span>
                        </span>
                        <ChevronRight className="size-3 shrink-0" />
                      </Button>
                    </DialogTrigger>
                    <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
                      <DialogHeader>
                        <DialogTitle>记录详情</DialogTitle>
                        <DialogDescription>当前记录的完整字段</DialogDescription>
                      </DialogHeader>
                      <FieldGroup className="gap-3">
                        <Field>
                          <FieldTitle>类型</FieldTitle>
                          <div className="min-w-0 break-words [&_*]:max-w-full">
                            {credit.source === "admin_reset" ? "管理员直接重置" : "重置卡"}
                          </div>
                        </Field>
                        <Field>
                          <FieldTitle>发放时间</FieldTitle>
                          <div className="min-w-0 break-words [&_*]:max-w-full">
                            {date(credit.granted_at)}
                          </div>
                        </Field>
                        <Field>
                          <FieldTitle>启用时间</FieldTitle>
                          <div className="min-w-0 break-words [&_*]:max-w-full">
                            {date(credit.available_at)}
                          </div>
                        </Field>
                        <Field>
                          <FieldTitle>到期时间</FieldTitle>
                          <div className="min-w-0 break-words [&_*]:max-w-full">
                            {date(credit.expires_at)}
                          </div>
                        </Field>
                        <Field>
                          <FieldTitle>状态</FieldTitle>
                          <div className="min-w-0 break-words [&_*]:max-w-full">
                            <Badge variant="outline">
                              {
                                {
                                  available: "可用",
                                  redeemed: "已使用",
                                  pending: "待启用",
                                  expired: "已过期",
                                  not_applied: "未执行",
                                }[credit.status]
                              }
                            </Badge>
                          </div>
                        </Field>
                        <Field>
                          <FieldTitle>使用时间</FieldTitle>
                          <div className="min-w-0 break-words [&_*]:max-w-full">
                            {date(credit.redeemed_at)}
                          </div>
                        </Field>
                        <Field>
                          <FieldTitle>使用方</FieldTitle>
                          <div className="min-w-0 break-words [&_*]:max-w-full">
                            {credit.redeemed_by === "admin"
                              ? "管理员"
                              : credit.redeemed_by === "client"
                                ? "客户端"
                                : "—"}
                          </div>
                        </Field>
                        <Field>
                          <FieldTitle>重置窗口数</FieldTitle>
                          <div className="min-w-0 break-words [&_*]:max-w-full">
                            {credit.status === "redeemed" ? credit.windows_reset : "—"}
                          </div>
                        </Field>
                        <Field>
                          <FieldTitle>管理备注</FieldTitle>
                          <div className="min-w-0 break-words [&_*]:max-w-full">
                            {credit.note || "—"}
                          </div>
                        </Field>
                      </FieldGroup>
                    </DialogContent>
                  </Dialog>
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("发放时间")}
                  className=" "
                  data-label="发放时间"
                  role="cell"
                >
                  {date(credit.granted_at)}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("启用时间")}
                  className=" "
                  data-label="启用时间"
                  role="cell"
                >
                  {date(credit.available_at)}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("到期时间")}
                  className=" "
                  data-label="到期时间"
                  role="cell"
                >
                  {date(credit.expires_at)}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("状态")}
                  className=" max-md:overflow-hidden"
                  data-label="状态"
                  data-compact="true"
                  role="cell"
                >
                  <Badge variant="outline">
                    {
                      {
                        available: "可用",
                        redeemed: "已使用",
                        pending: "待启用",
                        expired: "已过期",
                        not_applied: "未执行",
                      }[credit.status]
                    }
                  </Badge>
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("使用时间")}
                  className=" "
                  data-label="使用时间"
                  role="cell"
                >
                  {date(credit.redeemed_at)}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("使用方")}
                  className=" "
                  data-label="使用方"
                  data-compact="true"
                  role="cell"
                >
                  {credit.redeemed_by === "admin"
                    ? "管理员"
                    : credit.redeemed_by === "client"
                      ? "客户端"
                      : "—"}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("重置窗口数")}
                  className=" "
                  data-label="重置窗口数"
                  data-compact="true"
                  role="cell"
                >
                  {credit.status === "redeemed" ? credit.windows_reset : "—"}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("管理备注")}
                  data-label="管理备注"
                  role="cell"
                  className="max-w-64 truncate "
                  title={credit.note}
                >
                  {credit.note || "—"}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("操作")}
                  className=" max-md:[&_button]:h-7 max-md:[&_button]:px-1.5 max-md:[&_button]:text-xs max-md:[&_button]:gap-1 max-md:[&_a]:h-7 max-md:[&_a]:px-1.5 max-md:[&_a]:text-xs max-md:[&_a]:gap-1 max-md:[&>div]:gap-1"
                  data-label="操作"
                  role="cell"
                >
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={
                      !resource.ready ||
                      busy ||
                      credit.status !== "available" ||
                      credit.source !== "card"
                    }
                    onClick={() =>
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
                          if (result.code === "no_credit")
                            throw new Error("重置卡不可用，请刷新后重试。");
                        },
                        {
                          confirm:
                            "使用这张重置卡清零当前用量并重开额度周期？订阅到期时间及历史账单保持不变。",
                          success: "重置已完成",
                        },
                      )
                    }
                  >
                    使用
                  </Button>
                </TableCell>
              </TableRow>
            ))}
            {rows.rows.length === 0 && (
              <TableRow role="row">
                <TableCell
                  role="cell"
                  colSpan={tableColumns1.count}
                  className="text-center text-muted-foreground"
                >
                  {resource.loading
                    ? "正在加载重置卡"
                    : resource.data
                      ? "尚未发放重置卡"
                      : "重置卡记录暂不可用"}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </>
      <div className="flex items-center justify-end gap-2">
        <CardDescription>{rows.total} 张</CardDescription>
        <Button size="sm" variant="outline" {...rows.previous}>
          上一页
        </Button>
        <CardDescription>
          {rows.page} / {rows.pages}
        </CardDescription>
        <Button size="sm" variant="outline" {...rows.next}>
          下一页
        </Button>
      </div>
    </>
  );
}
function Routing({ account, id }: { account?: Consumer; id: string }) {
  const platformPrefix = usePlatformPrefix();
  const resource = useResource<RoutingResponse>(`${platformPrefix}/${id}/routing`);
  useErrorToast(resource.error);
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          供应绑定
        </CardTitle>
      </CardHeader>
      <CardContent>
        {
          <RoutingForm
            key={id}
            account={account}
            data={resource.data ?? { items: [] }}
            disabled={!resource.ready || !account}
            onSaved={resource.reload}
          />
        }
      </CardContent>
    </Card>
  );
}
function Devices({ id }: { id: string }) {
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
  return (
    <>
      {
        <>
          <Card size="sm">
            <CardHeader>
              <CardTitle role="heading" aria-level={2}>
                {"登录设备与授权范围"}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <>
                <div className="mb-2 flex justify-end">
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button type="button" variant="outline" size="sm" aria-label="显示列">
                        <Columns3 />
                        显示列
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-48">
                      <DropdownMenuLabel>
                        {tableColumns2.mobile ? "手机显示列" : "桌面显示列"}
                      </DropdownMenuLabel>
                      {tableColumns2.labels.map((label) => (
                        <DropdownMenuCheckboxItem
                          key={label}
                          checked={tableColumns2.isVisible(label)}
                          disabled={tableColumns2.count === 1 && tableColumns2.isVisible(label)}
                          onSelect={(event) => event.preventDefault()}
                          onCheckedChange={(checked) =>
                            tableColumns2.setVisible(label, checked === true)
                          }
                        >
                          {label}
                        </DropdownMenuCheckboxItem>
                      ))}
                      <DropdownMenuSeparator />
                      <DropdownMenuItem onSelect={tableColumns2.showAll}>
                        显示全部列
                      </DropdownMenuItem>
                      <DropdownMenuItem onSelect={tableColumns2.reset}>恢复默认列</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                <Table
                  className={
                    tableColumns2.count > 4
                      ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                      : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  }
                  role="table"
                >
                  <TableHeader>
                    <TableRow role="row">
                      {["客户端 / 安装标识", "授权范围", "首次登录", "最近续期 / 使用", "操作"].map(
                        (label) => (
                          <TableHead
                            hidden={!tableColumns2.isVisible(label)}
                            className={
                              ["客户端 / 安装标识", "最近续期 / 使用", "操作"].includes(label)
                                ? label === "操作"
                                  ? "max-md:w-28"
                                  : label === "客户端 / 安装标识"
                                    ? ""
                                    : "max-md:w-16"
                                : ""
                            }
                            key={label}
                            scope="col"
                          >
                            {label}
                          </TableHead>
                        ),
                      )}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {(resource.data?.items ?? []).length ? (
                      <>
                        {devices.rows.map((device) => (
                          <TableRow role="row" key={device.id}>
                            <TableCell
                              hidden={!tableColumns2.isVisible("客户端 / 安装标识")}
                              data-label="客户端 / 安装标识"
                              role="cell"
                              className="max-w-80 whitespace-normal break-words max-md:overflow-hidden"
                            >
                              <div className="max-md:hidden">
                                {device.user_agent || "未知客户端"}
                                <CardDescription className="break-all font-mono text-xs">
                                  {device.installation_id ?? "未提供安装标识"}
                                </CardDescription>
                              </div>
                              <Dialog>
                                <DialogTrigger asChild>
                                  <Button
                                    type="button"
                                    variant="ghost"
                                    className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                                    aria-label={
                                      "查看详情：" + String(device.user_agent || "未知客户端")
                                    }
                                  >
                                    <span className="min-w-0 flex-1">
                                      <span className="block truncate font-medium">
                                        {device.user_agent || "未知客户端"}
                                      </span>
                                      <span className="block truncate text-xs text-muted-foreground">
                                        {device.installation_id ?? "未提供安装标识"}
                                      </span>
                                    </span>
                                    <ChevronRight className="size-3 shrink-0" />
                                  </Button>
                                </DialogTrigger>
                                <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
                                  <DialogHeader>
                                    <DialogTitle>记录详情</DialogTitle>
                                    <DialogDescription>当前记录的完整字段</DialogDescription>
                                  </DialogHeader>
                                  <FieldGroup className="gap-3">
                                    <Field>
                                      <FieldTitle>客户端 / 安装标识</FieldTitle>
                                      <div className="min-w-0 break-words [&_*]:max-w-full">
                                        {device.user_agent || "未知客户端"}
                                        <CardDescription className="break-all font-mono text-xs">
                                          {device.installation_id ?? "未提供安装标识"}
                                        </CardDescription>
                                      </div>
                                    </Field>
                                    <Field>
                                      <FieldTitle>授权范围</FieldTitle>
                                      <div className="min-w-0 break-words [&_*]:max-w-full">
                                        {device.scopes
                                          .split(/\s+/)
                                          .map((scope) => scopes[scope] ?? scope)
                                          .join("、")}
                                      </div>
                                    </Field>
                                    <Field>
                                      <FieldTitle>首次登录</FieldTitle>
                                      <div className="min-w-0 break-words [&_*]:max-w-full">
                                        {date(device.authenticated_at_ms ?? device.created_at)}
                                      </div>
                                    </Field>
                                    <Field>
                                      <FieldTitle>最近续期 / 使用</FieldTitle>
                                      <div className="min-w-0 break-words [&_*]:max-w-full">
                                        {date(device.last_login_at)}
                                        <CardDescription>
                                          {date(device.last_used_at)}
                                        </CardDescription>
                                      </div>
                                    </Field>
                                  </FieldGroup>
                                </DialogContent>
                              </Dialog>
                            </TableCell>
                            <TableCell
                              hidden={!tableColumns2.isVisible("授权范围")}
                              className=" "
                              data-label="授权范围"
                              role="cell"
                            >
                              {device.scopes
                                .split(/\s+/)
                                .map((scope) => scopes[scope] ?? scope)
                                .join("、")}
                            </TableCell>
                            <TableCell
                              hidden={!tableColumns2.isVisible("首次登录")}
                              className=" "
                              data-label="首次登录"
                              role="cell"
                            >
                              {date(device.authenticated_at_ms ?? device.created_at)}
                            </TableCell>
                            <TableCell
                              hidden={!tableColumns2.isVisible("最近续期 / 使用")}
                              className=" max-md:overflow-hidden"
                              data-label="最近续期 / 使用"
                              role="cell"
                            >
                              {date(device.last_login_at)}
                              <CardDescription>{date(device.last_used_at)}</CardDescription>
                            </TableCell>
                            <TableCell
                              hidden={!tableColumns2.isVisible("操作")}
                              className=" max-md:[&_button]:h-7 max-md:[&_button]:px-1.5 max-md:[&_button]:text-xs max-md:[&_button]:gap-1 max-md:[&_a]:h-7 max-md:[&_a]:px-1.5 max-md:[&_a]:text-xs max-md:[&_a]:gap-1 max-md:[&>div]:gap-1"
                              data-label="操作"
                              role="cell"
                            >
                              <Button
                                type="button"
                                variant={true ? "destructive" : "outline"}
                                disabled={
                                  false || actions.isBusy("components\\consumers.tsx:action:17")
                                }
                                onClick={() =>
                                  void actions.run(
                                    "components\\consumers.tsx:action:17",
                                    async () => {
                                      await request(
                                        `${platformPrefix}/${id}/devices/${device.id}/revoke`,
                                        {
                                          method: "POST",
                                          body: {},
                                        },
                                      );
                                      resource.reload();
                                    },
                                    {
                                      confirm: "撤销此设备授权并使其下线？",
                                      danger: true,
                                      success: undefined,
                                    },
                                  )
                                }
                              >
                                撤销授权
                              </Button>
                            </TableCell>
                          </TableRow>
                        ))}
                      </>
                    ) : (
                      <TableRow role="row">
                        <TableCell role="cell" colSpan={tableColumns2.count}>
                          <Empty>
                            <EmptyDescription>{"暂无记录"}</EmptyDescription>
                          </Empty>
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              </>
              <Pagination aria-label="登录设备分页" className="mt-3 justify-end">
                <PaginationContent className="flex-wrap justify-end gap-1">
                  <PaginationItem>
                    <Select {...devices.size}>
                      <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper" side="bottom" align="end">
                        {[10, 20, 30, 50].map((size) => (
                          <SelectItem key={size} value={String(size)}>
                            {size} 条/页
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </PaginationItem>
                  <PaginationItem className="mr-2 text-xs text-muted-foreground">
                    共 {devices.total ?? "—"} 条 · {devices.pages ?? "—"} 页
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="首页"
                      {...devices.first}
                    >
                      <ChevronsLeft />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="上一页"
                      {...devices.previous}
                    >
                      <ChevronLeft />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Input className="h-7 w-14 text-center tabular-nums" {...devices.input} />
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="下一页"
                      {...devices.next}
                    >
                      <ChevronRight />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="末页"
                      {...devices.last}
                    >
                      <ChevronsRight />
                    </Button>
                  </PaginationItem>
                </PaginationContent>
              </Pagination>
            </CardContent>
          </Card>
          <Card size="sm">
            <CardHeader>
              <CardTitle role="heading" aria-level={2}>
                {"远程主机注册"}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <>
                <div className="mb-2 flex justify-end">
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button type="button" variant="outline" size="sm" aria-label="显示列">
                        <Columns3 />
                        显示列
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-48">
                      <DropdownMenuLabel>
                        {tableColumns3.mobile ? "手机显示列" : "桌面显示列"}
                      </DropdownMenuLabel>
                      {tableColumns3.labels.map((label) => (
                        <DropdownMenuCheckboxItem
                          key={label}
                          checked={tableColumns3.isVisible(label)}
                          disabled={tableColumns3.count === 1 && tableColumns3.isVisible(label)}
                          onSelect={(event) => event.preventDefault()}
                          onCheckedChange={(checked) =>
                            tableColumns3.setVisible(label, checked === true)
                          }
                        >
                          {label}
                        </DropdownMenuCheckboxItem>
                      ))}
                      <DropdownMenuSeparator />
                      <DropdownMenuItem onSelect={tableColumns3.showAll}>
                        显示全部列
                      </DropdownMenuItem>
                      <DropdownMenuItem onSelect={tableColumns3.reset}>恢复默认列</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                <Table
                  className={
                    tableColumns3.count > 4
                      ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                      : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  }
                  role="table"
                >
                  <TableHeader>
                    <TableRow role="row">
                      {recordColumns("remote_servers").map((label) => (
                        <TableHead
                          hidden={!tableColumns3.isVisible(label)}
                          className={
                            mobileRecordColumns("remote_servers").includes(label)
                              ? "max-md:w-1/2"
                              : ""
                          }
                          key={label}
                        >
                          {label}
                        </TableHead>
                      ))}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {(serverResource.data?.items ?? []).length ? (
                      recordRows("remote_servers", servers.rows).map((row) => (
                        <TableRow role="row" key={row.key}>
                          {row.cells.map((cell) => (
                            <TableCell
                              hidden={!tableColumns3.isVisible(cell.label)}
                              data-label={cell.label}
                              role="cell"
                              key={cell.label}
                              className={
                                mobileRecordColumns("remote_servers").includes(cell.label)
                                  ? "max-w-80 whitespace-normal break-words"
                                  : " max-w-80 whitespace-normal break-words"
                              }
                            >
                              <div className="max-md:hidden">
                                {cell.status ? (
                                  <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                    {cell.text}
                                  </Badge>
                                ) : (
                                  cell.text
                                )}
                              </div>
                              {cell.label === mobileRecordColumns("remote_servers")[0] ? (
                                <Dialog>
                                  <DialogTrigger asChild>
                                    <Button
                                      type="button"
                                      variant="ghost"
                                      className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                                      aria-label={"查看详情：" + String(cell.text)}
                                    >
                                      <span className="min-w-0 flex-1">
                                        <span className="block truncate font-medium">
                                          {cell.text}
                                        </span>
                                      </span>
                                      <ChevronRight className="size-3 shrink-0" />
                                    </Button>
                                  </DialogTrigger>
                                  <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
                                    <DialogHeader>
                                      <DialogTitle>记录详情</DialogTitle>
                                      <DialogDescription>当前记录的完整字段</DialogDescription>
                                    </DialogHeader>
                                    <FieldGroup className="gap-3">
                                      {row.cells.map((cell) => (
                                        <Field key={cell.label}>
                                          <FieldTitle>{cell.label}</FieldTitle>
                                          <div className="min-w-0 break-words">
                                            {cell.status ? (
                                              <Badge
                                                variant={cell.failed ? "destructive" : "secondary"}
                                              >
                                                {cell.text}
                                              </Badge>
                                            ) : (
                                              cell.text
                                            )}
                                          </div>
                                        </Field>
                                      ))}
                                    </FieldGroup>
                                  </DialogContent>
                                </Dialog>
                              ) : (
                                <div className="truncate md:hidden">
                                  {cell.status ? (
                                    <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                      {cell.text}
                                    </Badge>
                                  ) : (
                                    cell.text
                                  )}
                                </div>
                              )}
                            </TableCell>
                          ))}
                        </TableRow>
                      ))
                    ) : (
                      <TableRow role="row">
                        <TableCell role="cell" colSpan={tableColumns3.count}>
                          <Empty>
                            <EmptyDescription>暂无记录</EmptyDescription>
                          </Empty>
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              </>
              <Pagination aria-label="远程主机分页" className="mt-3 justify-end">
                <PaginationContent className="flex-wrap justify-end gap-1">
                  <PaginationItem>
                    <Select {...servers.size}>
                      <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper" side="bottom" align="end">
                        {[10, 20, 30, 50].map((size) => (
                          <SelectItem key={size} value={String(size)}>
                            {size} 条/页
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </PaginationItem>
                  <PaginationItem className="mr-2 text-xs text-muted-foreground">
                    共 {servers.total ?? "—"} 条 · {servers.pages ?? "—"} 页
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="首页"
                      {...servers.first}
                    >
                      <ChevronsLeft />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="上一页"
                      {...servers.previous}
                    >
                      <ChevronLeft />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Input className="h-7 w-14 text-center tabular-nums" {...servers.input} />
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="下一页"
                      {...servers.next}
                    >
                      <ChevronRight />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="末页"
                      {...servers.last}
                    >
                      <ChevronsRight />
                    </Button>
                  </PaginationItem>
                </PaginationContent>
              </Pagination>
            </CardContent>
          </Card>
        </>
      }
    </>
  );
}
function Records({ id, kind, title }: { id: string; kind: string; title: string }) {
  const platformPrefix = usePlatformPrefix();
  const tableColumns4 = useColumnVisibility(
    "components/consumers.tsx:4:" + kind,
    recordColumns(kind),
    mobileRecordColumns(kind),
  );

  const resource = useListResource<Json>(`${platformPrefix}/${id}/records`, { kind });
  const pagination = resource.pagination;
  useErrorToast(resource.error);
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {
          <>
            <div className="mb-2 flex justify-end">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button type="button" variant="outline" size="sm" aria-label="显示列">
                    <Columns3 />
                    显示列
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-48">
                  <DropdownMenuLabel>
                    {tableColumns4.mobile ? "手机显示列" : "桌面显示列"}
                  </DropdownMenuLabel>
                  {tableColumns4.labels.map((label) => (
                    <DropdownMenuCheckboxItem
                      key={label}
                      checked={tableColumns4.isVisible(label)}
                      disabled={tableColumns4.count === 1 && tableColumns4.isVisible(label)}
                      onSelect={(event) => event.preventDefault()}
                      onCheckedChange={(checked) =>
                        tableColumns4.setVisible(label, checked === true)
                      }
                    >
                      {label}
                    </DropdownMenuCheckboxItem>
                  ))}
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onSelect={tableColumns4.showAll}>显示全部列</DropdownMenuItem>
                  <DropdownMenuItem onSelect={tableColumns4.reset}>恢复默认列</DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
            <Table
              className={
                tableColumns4.count > 4
                  ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
              }
              role="table"
            >
              <TableHeader>
                <TableRow role="row">
                  {recordColumns(kind).map((label) => (
                    <TableHead
                      hidden={!tableColumns4.isVisible(label)}
                      className={mobileRecordColumns(kind).includes(label) ? "max-md:w-1/2" : ""}
                      key={label}
                    >
                      {label}
                    </TableHead>
                  ))}
                </TableRow>
              </TableHeader>
              <TableBody>
                {(resource.data?.items ?? []).length ? (
                  recordRows(kind, pagination.rows).map((row) => (
                    <TableRow role="row" key={row.key}>
                      {row.cells.map((cell) => (
                        <TableCell
                          hidden={!tableColumns4.isVisible(cell.label)}
                          data-label={cell.label}
                          role="cell"
                          key={cell.label}
                          className={
                            mobileRecordColumns(kind).includes(cell.label)
                              ? "max-w-80 whitespace-normal break-words"
                              : " max-w-80 whitespace-normal break-words"
                          }
                        >
                          <div className="max-md:hidden">
                            {cell.status ? (
                              <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                {cell.text}
                              </Badge>
                            ) : (
                              cell.text
                            )}
                          </div>
                          {cell.label === mobileRecordColumns(kind)[0] ? (
                            <Dialog>
                              <DialogTrigger asChild>
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                                  aria-label={"查看详情：" + String(cell.text)}
                                >
                                  <span className="min-w-0 flex-1">
                                    <span className="block truncate font-medium">{cell.text}</span>
                                  </span>
                                  <ChevronRight className="size-3 shrink-0" />
                                </Button>
                              </DialogTrigger>
                              <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
                                <DialogHeader>
                                  <DialogTitle>记录详情</DialogTitle>
                                  <DialogDescription>当前记录的完整字段</DialogDescription>
                                </DialogHeader>
                                <FieldGroup className="gap-3">
                                  {row.cells.map((cell) => (
                                    <Field key={cell.label}>
                                      <FieldTitle>{cell.label}</FieldTitle>
                                      <div className="min-w-0 break-words">
                                        {cell.status ? (
                                          <Badge
                                            variant={cell.failed ? "destructive" : "secondary"}
                                          >
                                            {cell.text}
                                          </Badge>
                                        ) : (
                                          cell.text
                                        )}
                                      </div>
                                    </Field>
                                  ))}
                                </FieldGroup>
                              </DialogContent>
                            </Dialog>
                          ) : (
                            <div className="truncate md:hidden">
                              {cell.status ? (
                                <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                  {cell.text}
                                </Badge>
                              ) : (
                                cell.text
                              )}
                            </div>
                          )}
                        </TableCell>
                      ))}
                    </TableRow>
                  ))
                ) : (
                  <TableRow role="row">
                    <TableCell role="cell" colSpan={tableColumns4.count}>
                      <Empty>
                        <EmptyDescription>暂无记录</EmptyDescription>
                      </Empty>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </>
        }
        <Pagination aria-label="记录分页" className="mt-3 justify-end">
          <PaginationContent className="flex-wrap justify-end gap-1">
            <PaginationItem>
              <Select {...pagination.size}>
                <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper" side="bottom" align="end">
                  {[10, 20, 30, 50].map((size) => (
                    <SelectItem key={size} value={String(size)}>
                      {size} 条/页
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </PaginationItem>
            <PaginationItem className="mr-2 text-xs text-muted-foreground">
              共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
            </PaginationItem>
            <PaginationItem>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="首页"
                {...pagination.first}
              >
                <ChevronsLeft />
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="上一页"
                {...pagination.previous}
              >
                <ChevronLeft />
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Input className="h-7 w-14 text-center tabular-nums" {...pagination.input} />
            </PaginationItem>
            <PaginationItem>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="下一页"
                {...pagination.next}
              >
                <ChevronRight />
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="末页"
                {...pagination.last}
              >
                <ChevronsRight />
              </Button>
            </PaginationItem>
          </PaginationContent>
        </Pagination>
      </CardContent>
    </Card>
  );
}
const recordKinds = [
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
function ClientRecords({ id }: { id: string }) {
  const fieldId = useId();
  const [kind, setKind] = useState("logs");
  const kinds = [
    ["logs", "活动日志"],
    ["cloud_environment", "任务关联的云环境"],
    ["subscription_operation", "订阅操作记录"],
    ...recordKinds,
    ["state", "客户端偏好与安装状态"],
  ];
  return (
    <>
      <div className="flex flex-wrap items-center gap-3">
        <Field orientation="horizontal" className="w-fit flex-wrap">
          <FieldLabel htmlFor={`${fieldId}-kind`}>记录类别</FieldLabel>
          <Select value={kind} onValueChange={setKind}>
            <SelectTrigger id={`${fieldId}-kind`} className="w-64">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              {kinds.map(([key, label]) => (
                <SelectItem key={key} value={key}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
      </div>
      {kind === "logs" ? (
        <Logs id={id} />
      ) : kind === "state" ? (
        <ClientStatePanel id={id} />
      ) : (
        <Records
          key={kind}
          id={id}
          kind={kind}
          title={kinds.find(([key]) => key === kind)?.[1] ?? "记录"}
        />
      )}
    </>
  );
}
function Logs({ id }: { id: string }) {
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
  return (
    <>
      <Card size="sm">
        <CardContent className="space-y-4">
          <form
            className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters });
              setPage(1);
              resource.reload();
            }}
          >
            <Field>
              <FieldLabel
                htmlFor={fieldId + "-field-19" + "-" + encodeURIComponent(String("请求接口"))}
              >
                {"请求接口"}
              </FieldLabel>
              <Input
                id={fieldId + "-field-19" + "-" + encodeURIComponent(String("请求接口"))}
                aria-label={"请求接口"}
                value={filters.path}
                onChange={(event) => setFilters({ ...filters, path: event.target.value })}
                placeholder="输入请求路径"
              />
            </Field>
            <Field>
              <FieldLabel
                htmlFor={fieldId + "-field-20" + "-" + encodeURIComponent(String("请求方法"))}
              >
                {"请求方法"}
              </FieldLabel>
              <Select
                value={filters.method}
                onValueChange={(next) =>
                  ((method) => setFilters({ ...filters, method }))(
                    next ===
                      fieldId +
                        "-field-20" +
                        "-" +
                        encodeURIComponent(String("请求方法")) +
                        "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-20" + "-" + encodeURIComponent(String("请求方法"))}
                  aria-label={"请求方法"}
                  data-required={false ? "true" : undefined}
                  data-empty={String(filters.method) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部方法" },
                        ...["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"].map((value) => ({
                          value,
                          label: value,
                        })),
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部方法" },
                    ...["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"].map((value) => ({
                      value,
                      label: value,
                    })),
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-20" +
                          "-" +
                          encodeURIComponent(String("请求方法")) +
                          "-empty"
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
              <FieldLabel
                htmlFor={fieldId + "-field-21" + "-" + encodeURIComponent(String("响应结果"))}
              >
                {"响应结果"}
              </FieldLabel>
              <Select
                value={filters.result}
                onValueChange={(next) =>
                  ((result) => setFilters({ ...filters, result }))(
                    next ===
                      fieldId +
                        "-field-21" +
                        "-" +
                        encodeURIComponent(String("响应结果")) +
                        "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-21" + "-" + encodeURIComponent(String("响应结果"))}
                  aria-label={"响应结果"}
                  data-required={false ? "true" : undefined}
                  data-empty={String(filters.result) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部结果" },
                        { value: "success", label: "成功（2xx / 3xx）" },
                        { value: "failed", label: "失败（4xx / 5xx）" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部结果" },
                    { value: "success", label: "成功（2xx / 3xx）" },
                    { value: "failed", label: "失败（4xx / 5xx）" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-21" +
                          "-" +
                          encodeURIComponent(String("响应结果")) +
                          "-empty"
                      }
                      disabled={"disabled" in option && Boolean(option.disabled)}
                    >
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <div className="flex flex-wrap items-center gap-2 self-end">
              <Button type="submit">
                <Search />
                查询
              </Button>
              <Button
                type="button"
                variant="secondary"
                onClick={() => {
                  setFilters(empty);
                  setApplied(empty);
                  setPage(1);
                  resource.reload();
                }}
              >
                <RotateCcw />
                重置
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      {
        <>
          <Card size="sm">
            <CardHeader>
              <CardTitle role="heading" aria-level={2}>
                {"请求日志"}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <>
                <div className="mb-2 flex justify-end">
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button type="button" variant="outline" size="sm" aria-label="显示列">
                        <Columns3 />
                        显示列
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-48">
                      <DropdownMenuLabel>
                        {tableColumns5.mobile ? "手机显示列" : "桌面显示列"}
                      </DropdownMenuLabel>
                      {tableColumns5.labels.map((label) => (
                        <DropdownMenuCheckboxItem
                          key={label}
                          checked={tableColumns5.isVisible(label)}
                          disabled={tableColumns5.count === 1 && tableColumns5.isVisible(label)}
                          onSelect={(event) => event.preventDefault()}
                          onCheckedChange={(checked) =>
                            tableColumns5.setVisible(label, checked === true)
                          }
                        >
                          {label}
                        </DropdownMenuCheckboxItem>
                      ))}
                      <DropdownMenuSeparator />
                      <DropdownMenuItem onSelect={tableColumns5.showAll}>
                        显示全部列
                      </DropdownMenuItem>
                      <DropdownMenuItem onSelect={tableColumns5.reset}>恢复默认列</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                <Table
                  className={
                    tableColumns5.count > 4
                      ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                      : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  }
                  role="table"
                >
                  <TableHeader>
                    <TableRow role="row">
                      {recordColumns("logs").map((label) => (
                        <TableHead
                          hidden={!tableColumns5.isVisible(label)}
                          className={
                            mobileRecordColumns("logs").includes(label) ? "max-md:w-1/2" : ""
                          }
                          key={label}
                        >
                          {label}
                        </TableHead>
                      ))}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {rows.length ? (
                      recordRows("logs", rows).map((row) => (
                        <TableRow role="row" key={row.key}>
                          {row.cells.map((cell) => (
                            <TableCell
                              hidden={!tableColumns5.isVisible(cell.label)}
                              data-label={cell.label}
                              role="cell"
                              key={cell.label}
                              className={
                                mobileRecordColumns("logs").includes(cell.label)
                                  ? "max-w-80 whitespace-normal break-words"
                                  : " max-w-80 whitespace-normal break-words"
                              }
                            >
                              <div className="max-md:hidden">
                                {cell.status ? (
                                  <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                    {cell.text}
                                  </Badge>
                                ) : (
                                  cell.text
                                )}
                              </div>
                              {cell.label === mobileRecordColumns("logs")[0] ? (
                                <Dialog>
                                  <DialogTrigger asChild>
                                    <Button
                                      type="button"
                                      variant="ghost"
                                      className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                                      aria-label={"查看详情：" + String(cell.text)}
                                    >
                                      <span className="min-w-0 flex-1">
                                        <span className="block truncate font-medium">
                                          {cell.text}
                                        </span>
                                      </span>
                                      <ChevronRight className="size-3 shrink-0" />
                                    </Button>
                                  </DialogTrigger>
                                  <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
                                    <DialogHeader>
                                      <DialogTitle>记录详情</DialogTitle>
                                      <DialogDescription>当前记录的完整字段</DialogDescription>
                                    </DialogHeader>
                                    <FieldGroup className="gap-3">
                                      {row.cells.map((cell) => (
                                        <Field key={cell.label}>
                                          <FieldTitle>{cell.label}</FieldTitle>
                                          <div className="min-w-0 break-words">
                                            {cell.status ? (
                                              <Badge
                                                variant={cell.failed ? "destructive" : "secondary"}
                                              >
                                                {cell.text}
                                              </Badge>
                                            ) : (
                                              cell.text
                                            )}
                                          </div>
                                        </Field>
                                      ))}
                                    </FieldGroup>
                                  </DialogContent>
                                </Dialog>
                              ) : (
                                <div className="truncate md:hidden">
                                  {cell.status ? (
                                    <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                      {cell.text}
                                    </Badge>
                                  ) : (
                                    cell.text
                                  )}
                                </div>
                              )}
                            </TableCell>
                          ))}
                        </TableRow>
                      ))
                    ) : (
                      <TableRow role="row">
                        <TableCell role="cell" colSpan={tableColumns5.count}>
                          <Empty>
                            <EmptyDescription>暂无记录</EmptyDescription>
                          </Empty>
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              </>
              <Pagination aria-label="记录分页" className="mt-3 justify-end">
                <PaginationContent className="flex-wrap justify-end gap-1">
                  <PaginationItem>
                    <Select {...pagination.size}>
                      <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper" side="bottom" align="end">
                        {[10, 20, 30, 50].map((size) => (
                          <SelectItem key={size} value={String(size)}>
                            {size} 条/页
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </PaginationItem>
                  <PaginationItem className="mr-2 text-xs text-muted-foreground">
                    共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="首页"
                      {...pagination.first}
                    >
                      <ChevronsLeft />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="上一页"
                      {...pagination.previous}
                    >
                      <ChevronLeft />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Input className="h-7 w-14 text-center tabular-nums" {...pagination.input} />
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="下一页"
                      {...pagination.next}
                    >
                      <ChevronRight />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="末页"
                      {...pagination.last}
                    >
                      <ChevronsRight />
                    </Button>
                  </PaginationItem>
                </PaginationContent>
              </Pagination>
            </CardContent>
          </Card>
          <div className="grid items-start gap-3 xl:grid-cols-2">
            <Card size="sm">
              <CardHeader>
                <CardTitle role="heading" aria-level={2}>
                  {"客户端活动"}
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <>
                  <div className="mb-2 flex justify-end">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button type="button" variant="outline" size="sm" aria-label="显示列">
                          <Columns3 />
                          显示列
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="w-48">
                        <DropdownMenuLabel>
                          {tableColumns6.mobile ? "手机显示列" : "桌面显示列"}
                        </DropdownMenuLabel>
                        {tableColumns6.labels.map((label) => (
                          <DropdownMenuCheckboxItem
                            key={label}
                            checked={tableColumns6.isVisible(label)}
                            disabled={tableColumns6.count === 1 && tableColumns6.isVisible(label)}
                            onSelect={(event) => event.preventDefault()}
                            onCheckedChange={(checked) =>
                              tableColumns6.setVisible(label, checked === true)
                            }
                          >
                            {label}
                          </DropdownMenuCheckboxItem>
                        ))}
                        <DropdownMenuSeparator />
                        <DropdownMenuItem onSelect={tableColumns6.showAll}>
                          显示全部列
                        </DropdownMenuItem>
                        <DropdownMenuItem onSelect={tableColumns6.reset}>
                          恢复默认列
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                  <Table
                    className={
                      tableColumns6.count > 4
                        ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                        : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                    }
                    role="table"
                  >
                    <TableHeader>
                      <TableRow role="row">
                        {recordColumns("analytics").map((label) => (
                          <TableHead
                            hidden={!tableColumns6.isVisible(label)}
                            className={
                              mobileRecordColumns("analytics").includes(label) ? "max-md:w-1/2" : ""
                            }
                            key={label}
                          >
                            {label}
                          </TableHead>
                        ))}
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {(analyticsResource.data?.items ?? []).length ? (
                        recordRows("analytics", analytics.rows).map((row) => (
                          <TableRow role="row" key={row.key}>
                            {row.cells.map((cell) => (
                              <TableCell
                                hidden={!tableColumns6.isVisible(cell.label)}
                                data-label={cell.label}
                                role="cell"
                                key={cell.label}
                                className={
                                  mobileRecordColumns("analytics").includes(cell.label)
                                    ? "max-w-80 whitespace-normal break-words"
                                    : " max-w-80 whitespace-normal break-words"
                                }
                              >
                                <div className="max-md:hidden">
                                  {cell.status ? (
                                    <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                      {cell.text}
                                    </Badge>
                                  ) : (
                                    cell.text
                                  )}
                                </div>
                                {cell.label === mobileRecordColumns("analytics")[0] ? (
                                  <Dialog>
                                    <DialogTrigger asChild>
                                      <Button
                                        type="button"
                                        variant="ghost"
                                        className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                                        aria-label={"查看详情：" + String(cell.text)}
                                      >
                                        <span className="min-w-0 flex-1">
                                          <span className="block truncate font-medium">
                                            {cell.text}
                                          </span>
                                        </span>
                                        <ChevronRight className="size-3 shrink-0" />
                                      </Button>
                                    </DialogTrigger>
                                    <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
                                      <DialogHeader>
                                        <DialogTitle>记录详情</DialogTitle>
                                        <DialogDescription>当前记录的完整字段</DialogDescription>
                                      </DialogHeader>
                                      <FieldGroup className="gap-3">
                                        {row.cells.map((cell) => (
                                          <Field key={cell.label}>
                                            <FieldTitle>{cell.label}</FieldTitle>
                                            <div className="min-w-0 break-words">
                                              {cell.status ? (
                                                <Badge
                                                  variant={
                                                    cell.failed ? "destructive" : "secondary"
                                                  }
                                                >
                                                  {cell.text}
                                                </Badge>
                                              ) : (
                                                cell.text
                                              )}
                                            </div>
                                          </Field>
                                        ))}
                                      </FieldGroup>
                                    </DialogContent>
                                  </Dialog>
                                ) : (
                                  <div className="truncate md:hidden">
                                    {cell.status ? (
                                      <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                        {cell.text}
                                      </Badge>
                                    ) : (
                                      cell.text
                                    )}
                                  </div>
                                )}
                              </TableCell>
                            ))}
                          </TableRow>
                        ))
                      ) : (
                        <TableRow role="row">
                          <TableCell role="cell" colSpan={tableColumns6.count}>
                            <Empty>
                              <EmptyDescription>暂无记录</EmptyDescription>
                            </Empty>
                          </TableCell>
                        </TableRow>
                      )}
                    </TableBody>
                  </Table>
                </>
                <Pagination aria-label="活动分页" className="mt-3 justify-end">
                  <PaginationContent className="flex-wrap justify-end gap-1">
                    <PaginationItem>
                      <Select {...analytics.size}>
                        <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent position="popper" side="bottom" align="end">
                          {[10, 20, 30, 50].map((size) => (
                            <SelectItem key={size} value={String(size)}>
                              {size} 条/页
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </PaginationItem>
                    <PaginationItem className="mr-2 text-xs text-muted-foreground">
                      共 {analytics.total ?? "—"} 条 · {analytics.pages ?? "—"} 页
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="首页"
                        {...analytics.first}
                      >
                        <ChevronsLeft />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="上一页"
                        {...analytics.previous}
                      >
                        <ChevronLeft />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Input className="h-7 w-14 text-center tabular-nums" {...analytics.input} />
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="下一页"
                        {...analytics.next}
                      >
                        <ChevronRight />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="末页"
                        {...analytics.last}
                      >
                        <ChevronsRight />
                      </Button>
                    </PaginationItem>
                  </PaginationContent>
                </Pagination>
              </CardContent>
            </Card>
            <Card size="sm">
              <CardHeader>
                <CardTitle role="heading" aria-level={2}>
                  {"网站策略查询"}
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <>
                  <div className="mb-2 flex justify-end">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button type="button" variant="outline" size="sm" aria-label="显示列">
                          <Columns3 />
                          显示列
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="w-48">
                        <DropdownMenuLabel>
                          {tableColumns7.mobile ? "手机显示列" : "桌面显示列"}
                        </DropdownMenuLabel>
                        {tableColumns7.labels.map((label) => (
                          <DropdownMenuCheckboxItem
                            key={label}
                            checked={tableColumns7.isVisible(label)}
                            disabled={tableColumns7.count === 1 && tableColumns7.isVisible(label)}
                            onSelect={(event) => event.preventDefault()}
                            onCheckedChange={(checked) =>
                              tableColumns7.setVisible(label, checked === true)
                            }
                          >
                            {label}
                          </DropdownMenuCheckboxItem>
                        ))}
                        <DropdownMenuSeparator />
                        <DropdownMenuItem onSelect={tableColumns7.showAll}>
                          显示全部列
                        </DropdownMenuItem>
                        <DropdownMenuItem onSelect={tableColumns7.reset}>
                          恢复默认列
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                  <Table
                    className={
                      tableColumns7.count > 4
                        ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                        : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                    }
                    role="table"
                  >
                    <TableHeader>
                      <TableRow role="row">
                        {recordColumns("site_status").map((label) => (
                          <TableHead
                            hidden={!tableColumns7.isVisible(label)}
                            className={
                              mobileRecordColumns("site_status").includes(label)
                                ? "max-md:w-1/2"
                                : ""
                            }
                            key={label}
                          >
                            {label}
                          </TableHead>
                        ))}
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {(sitesResource.data?.items ?? []).length ? (
                        recordRows("site_status", sites.rows).map((row) => (
                          <TableRow role="row" key={row.key}>
                            {row.cells.map((cell) => (
                              <TableCell
                                hidden={!tableColumns7.isVisible(cell.label)}
                                data-label={cell.label}
                                role="cell"
                                key={cell.label}
                                className={
                                  mobileRecordColumns("site_status").includes(cell.label)
                                    ? "max-w-80 whitespace-normal break-words"
                                    : " max-w-80 whitespace-normal break-words"
                                }
                              >
                                <div className="max-md:hidden">
                                  {cell.status ? (
                                    <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                      {cell.text}
                                    </Badge>
                                  ) : (
                                    cell.text
                                  )}
                                </div>
                                {cell.label === mobileRecordColumns("site_status")[0] ? (
                                  <Dialog>
                                    <DialogTrigger asChild>
                                      <Button
                                        type="button"
                                        variant="ghost"
                                        className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                                        aria-label={"查看详情：" + String(cell.text)}
                                      >
                                        <span className="min-w-0 flex-1">
                                          <span className="block truncate font-medium">
                                            {cell.text}
                                          </span>
                                        </span>
                                        <ChevronRight className="size-3 shrink-0" />
                                      </Button>
                                    </DialogTrigger>
                                    <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
                                      <DialogHeader>
                                        <DialogTitle>记录详情</DialogTitle>
                                        <DialogDescription>当前记录的完整字段</DialogDescription>
                                      </DialogHeader>
                                      <FieldGroup className="gap-3">
                                        {row.cells.map((cell) => (
                                          <Field key={cell.label}>
                                            <FieldTitle>{cell.label}</FieldTitle>
                                            <div className="min-w-0 break-words">
                                              {cell.status ? (
                                                <Badge
                                                  variant={
                                                    cell.failed ? "destructive" : "secondary"
                                                  }
                                                >
                                                  {cell.text}
                                                </Badge>
                                              ) : (
                                                cell.text
                                              )}
                                            </div>
                                          </Field>
                                        ))}
                                      </FieldGroup>
                                    </DialogContent>
                                  </Dialog>
                                ) : (
                                  <div className="truncate md:hidden">
                                    {cell.status ? (
                                      <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                        {cell.text}
                                      </Badge>
                                    ) : (
                                      cell.text
                                    )}
                                  </div>
                                )}
                              </TableCell>
                            ))}
                          </TableRow>
                        ))
                      ) : (
                        <TableRow role="row">
                          <TableCell role="cell" colSpan={tableColumns7.count}>
                            <Empty>
                              <EmptyDescription>暂无记录</EmptyDescription>
                            </Empty>
                          </TableCell>
                        </TableRow>
                      )}
                    </TableBody>
                  </Table>
                </>
                <Pagination aria-label="网站策略分页" className="mt-3 justify-end">
                  <PaginationContent className="flex-wrap justify-end gap-1">
                    <PaginationItem>
                      <Select {...sites.size}>
                        <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent position="popper" side="bottom" align="end">
                          {[10, 20, 30, 50].map((size) => (
                            <SelectItem key={size} value={String(size)}>
                              {size} 条/页
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </PaginationItem>
                    <PaginationItem className="mr-2 text-xs text-muted-foreground">
                      共 {sites.total ?? "—"} 条 · {sites.pages ?? "—"} 页
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="首页"
                        {...sites.first}
                      >
                        <ChevronsLeft />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="上一页"
                        {...sites.previous}
                      >
                        <ChevronLeft />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Input className="h-7 w-14 text-center tabular-nums" {...sites.input} />
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="下一页"
                        {...sites.next}
                      >
                        <ChevronRight />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="末页"
                        {...sites.last}
                      >
                        <ChevronsRight />
                      </Button>
                    </PaginationItem>
                  </PaginationContent>
                </Pagination>
              </CardContent>
            </Card>
          </div>
        </>
      }
    </>
  );
}
