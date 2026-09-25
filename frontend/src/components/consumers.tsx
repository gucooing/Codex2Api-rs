"use client";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useTablePagination, usePageControls } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight } from "lucide-react";

import { useDialogFocus } from "@/lib/actions";
import { ScrollArea } from "@/components/ui/scroll-area";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
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
import {
  Combobox,
  ComboboxInput,
  ComboboxContent,
  ComboboxList,
  ComboboxItem,
  ComboboxEmpty,
} from "@/components/ui/combobox";
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
  type Plans,
  type Supplier,
  type Device,
  type Json,
} from "@/lib/api";
import { localDate, scopes } from "@/lib/domain";
import { ResourceRefreshContext, useQueryId, useResource } from "@/lib/hooks";

import { ConsumerUsage } from "./usage";
import { ConfigPanel, ClientStatePanel } from "./config";
import { accountConfigGroups } from "@/lib/account-fields";
import { recordColumns, recordRows } from "@/lib/records";

const subscriptionLabel = (status: Consumer["subscription_status"]) =>
  ({ active: "订阅有效", expired: "订阅已到期", free: "免费层" })[status];
export function ConsumersPage() {
  const dialogFocus = useDialogFocus();

  const fieldId = useId();
  const actions = useActions();
  const [create, setCreate] = useState(false);
  const empty = { search: "", status: "", subscription: "" };
  const [filters, setFilters] = useState(empty);
  const [applied, setApplied] = useState(empty);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const resource = useResource<
    List<Consumer & { quota: { windows: SupplierQuotaWindow[] } }> & {
      total: number;
      page: number;
      page_size: number;
    }
  >(`/consumers${query({ page, page_size: pageSize, ...applied })}`);
  const [view, setView] = useState<"table" | "cards">("table");
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
            <Field className="w-44">
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
            <Field className="w-44">
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
            <Field className="w-44">
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
            <ToggleGroup
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
          {view === "table" ? (
            <Table>
              <TableHeader>
                <TableRow>
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
                    <TableHead key={label} scope="col">
                      {label}
                    </TableHead>
                  ))}
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.length ? (
                  <>
                    {items.map((account) => (
                      <TableRow key={account.id}>
                        <TableCell>
                          <Checkbox
                            aria-label={`选择 ${account.name}`}
                            checked={isSelected(account.id)}
                            disabled={!resource.ready || batchBusy}
                            onCheckedChange={(checked) => toggleIds([account.id], checked === true)}
                          />
                        </TableCell>
                        <TableCell>
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
                        </TableCell>
                        <TableCell>{account.provider_id}</TableCell>
                        <TableCell>
                          {account.plan_name}
                          <CardDescription>
                            {subscriptionLabel(account.subscription_status)}
                          </CardDescription>
                        </TableCell>
                        <TableCell>
                          {account.subscription_expires_at
                            ? date(account.subscription_expires_at)
                            : "未设置到期时间"}
                        </TableCell>
                        <TableCell>
                          <Badge variant={account.enabled ? "secondary" : "outline"}>
                            {account.enabled ? "已启用" : "已停用"}
                          </Badge>
                        </TableCell>
                        <TableCell>
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
                        <TableCell>{accountActions(account)}</TableCell>
                      </TableRow>
                    ))}
                  </>
                ) : (
                  <TableRow>
                    <TableCell
                      colSpan={
                        [
                          "选择",
                          "虚拟账户",
                          "提供商",
                          "当前权益",
                          "订阅到期",
                          "登录状态",
                          "额度",
                          "操作",
                        ].length
                      }
                    >
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
                      <SelectContent>
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
  const plans = useResource<Plans>("/plans");
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
    ...changes,
  };
  const update = <K extends keyof ConsumerWrite>(key: K, next: ConsumerWrite[K]) =>
    setChanges((current) => ({ ...current, [key]: next }));
  const busy = actions.isBusy("consumer-account");
  const planOptions =
    plans.data?.items.filter(
      (plan) =>
        plan.provider_id === value.provider_id && (plan.enabled || plan.id === value.plan_id),
    ) ?? [];
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
            <FieldDescription>与订阅是否到期分别管理。</FieldDescription>
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
                  {plan.enabled ? "" : "（停止新分配）"}
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
      <FieldDescription>到期结束付费权益，免费访问和额度由所选套餐管理。</FieldDescription>
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
export function ConsumerDetail() {
  const id = useQueryId();
  const fieldId = useId();
  const resource = useResource<Consumer>(id ? `/consumers/${encodeURIComponent(id)}` : null);
  const [tab, setTab] = useState("settings");
  const [group, setGroup] = useState("account");
  const [refreshVersion, setRefreshVersion] = useState(0);
  useErrorToast(!id ? "缺少虚拟账户编号。" : undefined);
  useErrorToast(resource.error);
  if (!id)
    return (
      <Empty>
        <EmptyDescription>请选择虚拟账户</EmptyDescription>
        <Button asChild variant="outline">
          <Link href="/consumers/">返回列表</Link>
        </Button>
      </Empty>
    );
  const account = resource.data;
  return (
    <ResourceRefreshContext value={refreshVersion}>
      <Tabs value={tab} onValueChange={setTab} className="min-w-0 gap-3">
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2">
          <div className="min-w-0 max-w-full flex-1 overflow-x-auto overflow-y-hidden pb-1">
            <TabsList variant="line" aria-label="虚拟账户详情">
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
              <Link href="/consumers/">返回列表</Link>
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
                    {accountConfigGroups.map((item) => (
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
                      <ConsumerForm
                        key={id}
                        editing
                        disabled={!resource.ready}
                        account={account}
                        onSaved={resource.reload}
                      />
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
  const fieldId = useId();
  const dialogFocus = useDialogFocus();
  const [grantOpen, setGrantOpen] = useState(false);
  const [startMode, setStartMode] = useState("now");
  const path = `/consumers/${encodeURIComponent(id)}/reset-credits`;
  const resource = useResource<List<ResetCreditRecord> & { available_count: number }>(path);
  const actions = useActions();
  const [quantity, setQuantity] = useState("1");
  const [note, setNote] = useState("");
  const [activateAt, setActivateAt] = useState("");
  const [durationDays, setDurationDays] = useState("30");
  const grantAttempt = useRef<{ signature: string; id: string } | null>(null);
  const consumeAttempts = useRef(new Map<string, string>());
  const busy = actions.isBusy(`reset-credits-${id}`);
  const rows = useTablePagination(resource.data?.items ?? [], id, resource.data !== undefined);
  useErrorToast(resource.error);
  return (
    <>
      <div className="flex items-center gap-3">
        <Button
          size="sm"
          disabled={!resource.ready || busy}
          onClick={() => {
            if (!grantAttempt.current) {
              setStartMode("now");
              setActivateAt("");
              setDurationDays("30");
              setQuantity("1");
              setNote("");
            }
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
                  const signature = JSON.stringify([
                    quantity,
                    note.trim(),
                    startMode,
                    activateAt,
                    durationDays,
                  ]);
                  if (grantAttempt.current?.signature !== signature)
                    grantAttempt.current = { signature, id: crypto.randomUUID() };
                  await request(path, {
                    method: "POST",
                    body: {
                      request_id: grantAttempt.current.id,
                      quantity: Number(quantity),
                      note: note.trim(),
                      activate_at:
                        startMode === "scheduled" ? new Date(activateAt).toISOString() : null,
                      duration_days: Number(durationDays),
                    },
                  });
                  grantAttempt.current = null;
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
                <SelectContent>
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
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>类型</TableHead>
            <TableHead>发放时间</TableHead>
            <TableHead>启用时间</TableHead>
            <TableHead>到期时间</TableHead>
            <TableHead>状态</TableHead>
            <TableHead>使用时间</TableHead>
            <TableHead>使用方</TableHead>
            <TableHead>重置窗口数</TableHead>
            <TableHead>管理备注</TableHead>
            <TableHead>操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.rows.map((credit) => (
            <TableRow key={credit.id}>
              <TableCell>{credit.source === "admin_reset" ? "管理员直接重置" : "重置卡"}</TableCell>
              <TableCell>{date(credit.granted_at)}</TableCell>
              <TableCell>{date(credit.available_at)}</TableCell>
              <TableCell>{date(credit.expires_at)}</TableCell>
              <TableCell>
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
              <TableCell>{date(credit.redeemed_at)}</TableCell>
              <TableCell>
                {credit.redeemed_by === "admin"
                  ? "管理员"
                  : credit.redeemed_by === "client"
                    ? "客户端"
                    : "—"}
              </TableCell>
              <TableCell>{credit.status === "redeemed" ? credit.windows_reset : "—"}</TableCell>
              <TableCell className="max-w-64 truncate" title={credit.note}>
                {credit.note || "—"}
              </TableCell>
              <TableCell>
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
                        let attempt = consumeAttempts.current.get(credit.id);
                        if (!attempt) {
                          attempt = crypto.randomUUID();
                          consumeAttempts.current.set(credit.id, attempt);
                        }
                        const result = await request<{ code: string }>(`${path}/consume`, {
                          method: "POST",
                          body: {
                            credit_id: credit.id,
                            redeem_request_id: attempt,
                          },
                        });
                        consumeAttempts.current.delete(credit.id);
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
            <TableRow>
              <TableCell colSpan={10} className="text-center text-muted-foreground">
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
type RoutingResponse = {
  items: {
    virtual_account_id: string;
    provider_id: string;
    supplier_account_id: string | null;
    revision: number;
  }[];
};
function Routing({ account, id }: { account?: Consumer; id: string }) {
  const resource = useResource<RoutingResponse>(`/consumers/${id}/routing`);
  useErrorToast(resource.error);
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          供应绑定
        </CardTitle>
        <CardDescription>
          选择同提供商的供应账户；更换绑定保留账户身份、订阅和历史。
        </CardDescription>
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
function RoutingForm({
  account,
  data,
  onSaved,
  disabled,
}: {
  account?: Consumer;
  disabled: boolean;
  data: RoutingResponse;
  onSaved: () => void;
}) {
  const fieldId = useId();
  const actions = useActions();
  const route = data.items.find((route) => route.provider_id === account?.provider_id);
  const [supplierDraft, setSupplier] = useState<string>();
  const supplier = supplierDraft ?? route?.supplier_account_id ?? "";
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Supplier>();
  const current = useResource<Supplier>(
    supplier && selected?.id !== supplier ? `/suppliers/${encodeURIComponent(supplier)}` : null,
  );
  const suppliers = useResource<List<Supplier>>(
    open && account
      ? `/suppliers${query({ search: search.trim(), limit: 5, provider_id: account.provider_id, for_routing: true })}`
      : null,
    search.trim() ? 250 : 0,
  );
  const selectedAccount = selected?.id === supplier ? selected : current.data;
  useErrorToast(current.error);
  useErrorToast(suppliers.error);

  return (
    <form
      noValidate
      className="flex min-h-0 flex-col gap-4"
      aria-busy={actions.isBusy("components\\consumers.tsx:form:14")}
      onSubmit={(event) =>
        actions.submit(
          event,
          "components\\consumers.tsx:form:14",
          async () => {
            if (disabled || !account) throw new Error("请先加载执行路由");
            await request(`/consumers/${account.id}/routing`, {
              method: "PUT",
              body: { supplier_id: supplier || null, revision: route?.revision ?? null },
            });
            onSaved();
          },
          "已保存",
        )
      }
    >
      <FieldSet
        disabled={disabled || actions.isBusy("components\\consumers.tsx:form:14")}
        className="min-w-0"
      >
        <Field>
          <FieldLabel
            htmlFor={fieldId + "-field-16" + "-" + encodeURIComponent(String("执行供应账户"))}
          >
            {"执行供应账户"}
          </FieldLabel>
          <Combobox<Supplier>
            items={suppliers.data?.items ?? []}
            value={selectedAccount ?? null}
            onValueChange={(item) => {
              setSupplier(item?.id ?? "");
              setSelected(item ?? undefined);
              setSearch("");
            }}
            itemToStringLabel={(item) => item.display_name || item.email || item.id}
            itemToStringValue={(item) => item.id}
            isItemEqualToValue={(item, value) => item.id === value.id}
            filter={null}
            open={open}
            onOpenChange={(next, details) => {
              setOpen(next);
              if (next && details.reason !== "input-change") setSearch("");
            }}
            onInputValueChange={(text, details) => {
              if (details.reason === "input-change") {
                setSearch(text);
                if (!text) {
                  setSupplier("");
                  setSelected(undefined);
                }
              }
            }}
          >
            <ComboboxInput
              id={fieldId + "-field-16" + "-" + encodeURIComponent("执行供应账户")}
              aria-label="执行供应账户"
              placeholder="不提供执行服务"
              showClear
              className="w-full"
            />
            <ComboboxContent>
              <ComboboxEmpty>
                {suppliers.loading
                  ? "正在加载…"
                  : suppliers.error
                    ? "加载失败，请重新搜索"
                    : "没有匹配账户"}
              </ComboboxEmpty>
              <ComboboxList aria-busy={suppliers.loading}>
                {(item: Supplier) => (
                  <ComboboxItem key={item.id} value={item}>
                    <span className="flex min-w-0 flex-col">
                      <span className="truncate">
                        {item.display_name || item.email || item.id}
                        {item.status === "active" ? "" : "（未启用）"}
                      </span>
                      <span className="truncate text-xs text-muted-foreground">{item.email}</span>
                    </span>
                  </ComboboxItem>
                )}
              </ComboboxList>
            </ComboboxContent>
          </Combobox>
        </Field>
      </FieldSet>
      <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
        <Button
          type="submit"
          disabled={disabled || actions.isBusy("components\\consumers.tsx:form:14")}
        >
          {actions.isBusy("components\\consumers.tsx:form:14") && <Spinner />}
          {actions.isBusy("components\\consumers.tsx:form:14") ? "正在提交…" : "保存供应绑定"}
        </Button>
      </FieldGroup>
    </form>
  );
}
function Devices({ id }: { id: string }) {
  const actions = useActions();
  const resource = useResource<List<Device> & { remote_servers: Json[] }>(
    `/consumers/${id}/devices`,
  );
  const devices = useTablePagination(resource.data?.items ?? [], id, resource.data !== undefined);
  const servers = useTablePagination(
    resource.data?.remote_servers ?? [],
    id,
    resource.data !== undefined,
  );
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
              <CardDescription className="text-sm text-muted-foreground">
                授权只用于当前账户的消费操作。撤销后该设备需要重新登录。
              </CardDescription>
              <Table>
                <TableHeader>
                  <TableRow>
                    {["客户端 / 安装标识", "授权范围", "首次登录", "最近续期 / 使用", "操作"].map(
                      (label) => (
                        <TableHead key={label} scope="col">
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
                        <TableRow key={device.id}>
                          <TableCell className="max-w-80 whitespace-normal break-words">
                            {device.user_agent || "未知客户端"}
                            <CardDescription className="break-all font-mono text-xs">
                              {device.installation_id ?? "未提供安装标识"}
                            </CardDescription>
                          </TableCell>
                          <TableCell>
                            {device.scopes
                              .split(/\s+/)
                              .map((scope) => scopes[scope] ?? scope)
                              .join("、")}
                          </TableCell>
                          <TableCell>
                            {date(device.authenticated_at_ms ?? device.created_at)}
                          </TableCell>
                          <TableCell>
                            {date(device.last_login_at)}
                            <CardDescription>{date(device.last_used_at)}</CardDescription>
                          </TableCell>
                          <TableCell>
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
                                    await request(`/consumers/${id}/devices/${device.id}/revoke`, {
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
                    <TableRow>
                      <TableCell
                        colSpan={
                          ["客户端 / 安装标识", "授权范围", "首次登录", "最近续期 / 使用", "操作"]
                            .length
                        }
                      >
                        <Empty>
                          <EmptyDescription>{"暂无记录"}</EmptyDescription>
                        </Empty>
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
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
              <Table>
                <TableHeader>
                  <TableRow>
                    {recordColumns("remote_servers").map((label) => (
                      <TableHead key={label}>{label}</TableHead>
                    ))}
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {(resource.data?.remote_servers ?? []).length ? (
                    recordRows("remote_servers", servers.rows).map((row) => (
                      <TableRow key={row.key}>
                        {row.cells.map((cell) => (
                          <TableCell
                            key={cell.label}
                            className="max-w-80 whitespace-normal break-words"
                          >
                            {cell.status ? (
                              <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                {cell.text}
                              </Badge>
                            ) : (
                              cell.text
                            )}
                          </TableCell>
                        ))}
                      </TableRow>
                    ))
                  ) : (
                    <TableRow>
                      <TableCell colSpan={recordColumns("remote_servers").length}>
                        <Empty>
                          <EmptyDescription>暂无记录</EmptyDescription>
                        </Empty>
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
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
  const resource = useResource<List<Json>>(`/consumers/${id}/records${query({ kind })}`);
  const pagination = useTablePagination(
    resource.data?.items ?? [],
    `${id}/${kind}`,
    resource.data !== undefined,
  );
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
          <Table>
            <TableHeader>
              <TableRow>
                {recordColumns(kind).map((label) => (
                  <TableHead key={label}>{label}</TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {(resource.data?.items ?? []).length ? (
                recordRows(kind, pagination.rows).map((row) => (
                  <TableRow key={row.key}>
                    {row.cells.map((cell) => (
                      <TableCell
                        key={cell.label}
                        className="max-w-80 whitespace-normal break-words"
                      >
                        {cell.status ? (
                          <Badge variant={cell.failed ? "destructive" : "secondary"}>
                            {cell.text}
                          </Badge>
                        ) : (
                          cell.text
                        )}
                      </TableCell>
                    ))}
                  </TableRow>
                ))
              ) : (
                <TableRow>
                  <TableCell colSpan={recordColumns(kind).length}>
                    <Empty>
                      <EmptyDescription>暂无记录</EmptyDescription>
                    </Empty>
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
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
        <CardDescription>仅展示本账户的实际记录。</CardDescription>
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
  const fieldId = useId();
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const empty = { path: "", method: "", result: "" };
  const [filters, setFilters] = useState(empty);
  const [applied, setApplied] = useState(empty);
  const resource = useResource<
    List<Json> & {
      total: number;
      page: number;
      page_size: number;
      analytics: Json[];
      site_status: Json[];
    }
  >(`/consumers/${id}/logs${query({ page, page_size: pageSize, ...applied })}`);
  const rows = resource.data?.items ?? [];
  const pagination = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    setPage,
    pageSize,
    resource.refreshing,
    setPageSize,
  );
  const analytics = useTablePagination(
    resource.data?.analytics ?? [],
    id,
    resource.data !== undefined,
  );
  const sites = useTablePagination(
    resource.data?.site_status ?? [],
    id,
    resource.data !== undefined,
  );
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
              <Table>
                <TableHeader>
                  <TableRow>
                    {recordColumns("logs").map((label) => (
                      <TableHead key={label}>{label}</TableHead>
                    ))}
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.length ? (
                    recordRows("logs", rows).map((row) => (
                      <TableRow key={row.key}>
                        {row.cells.map((cell) => (
                          <TableCell
                            key={cell.label}
                            className="max-w-80 whitespace-normal break-words"
                          >
                            {cell.status ? (
                              <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                {cell.text}
                              </Badge>
                            ) : (
                              cell.text
                            )}
                          </TableCell>
                        ))}
                      </TableRow>
                    ))
                  ) : (
                    <TableRow>
                      <TableCell colSpan={recordColumns("logs").length}>
                        <Empty>
                          <EmptyDescription>暂无记录</EmptyDescription>
                        </Empty>
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
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
                <Table>
                  <TableHeader>
                    <TableRow>
                      {recordColumns("analytics").map((label) => (
                        <TableHead key={label}>{label}</TableHead>
                      ))}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {(resource.data?.analytics ?? []).length ? (
                      recordRows("analytics", analytics.rows).map((row) => (
                        <TableRow key={row.key}>
                          {row.cells.map((cell) => (
                            <TableCell
                              key={cell.label}
                              className="max-w-80 whitespace-normal break-words"
                            >
                              {cell.status ? (
                                <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                  {cell.text}
                                </Badge>
                              ) : (
                                cell.text
                              )}
                            </TableCell>
                          ))}
                        </TableRow>
                      ))
                    ) : (
                      <TableRow>
                        <TableCell colSpan={recordColumns("analytics").length}>
                          <Empty>
                            <EmptyDescription>暂无记录</EmptyDescription>
                          </Empty>
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
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
                <Table>
                  <TableHeader>
                    <TableRow>
                      {recordColumns("site_status").map((label) => (
                        <TableHead key={label}>{label}</TableHead>
                      ))}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {(resource.data?.site_status ?? []).length ? (
                      recordRows("site_status", sites.rows).map((row) => (
                        <TableRow key={row.key}>
                          {row.cells.map((cell) => (
                            <TableCell
                              key={cell.label}
                              className="max-w-80 whitespace-normal break-words"
                            >
                              {cell.status ? (
                                <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                  {cell.text}
                                </Badge>
                              ) : (
                                cell.text
                              )}
                            </TableCell>
                          ))}
                        </TableRow>
                      ))
                    ) : (
                      <TableRow>
                        <TableCell colSpan={recordColumns("site_status").length}>
                          <Empty>
                            <EmptyDescription>暂无记录</EmptyDescription>
                          </Empty>
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
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
