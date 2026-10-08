"use client";
import type { ProxyOption } from "@/lib/api";
import { query } from "@/lib/api";
import type { SupplierSelection } from "@/lib/supplier-selection";
import { SupplierTagEditor, SupplierTagsBatchDialog } from "@/components/supplier-tags";
import { selectedSupplierProvider } from "@/lib/supplier-selection";
import { Checkbox } from "@/components/ui/checkbox";
import { supplierSubscriptionLabel as subscriptionLabel } from "@/lib/subscriptions";
import type { SupplierTag } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useListResource } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight, Columns3 } from "lucide-react";

import { useDialogFocus, validateForm } from "@/lib/actions";
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
import { MoreHorizontal, Table2, LayoutGrid, Inbox, X, Copy } from "lucide-react";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import {
  Field,
  FieldLabel,
  FieldDescription,
  FieldGroup,
  FieldTitle,
  FieldSet,
  FieldContent,
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
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { useActions, useErrorToast, copyElementText } from "@/lib/actions";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DialogDescription,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { Progress } from "@/components/ui/progress";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { date } from "@/lib/format";
import { tokenCount } from "@/lib/usage-display";
import { Separator } from "@/components/ui/separator";
import Link from "next/link";
import { useState, useRef } from "react";
import { useQuotaClock } from "@/hooks/use-supplier-quotas";
import {
  supplierStatusLabel,
  quotaWindowLabel,
  quotaResetLabel,
  cycleUsageLabel,
  cycleUsageTitle,
  percentLabel,
} from "@/lib/supplier-state";

import { Plus, Search, RotateCcw, ArrowRight } from "lucide-react";
import { request, type Fingerprint, type List, type OAuth, type Supplier } from "@/lib/api";
import { mergeOAuth } from "@/lib/domain";
import { parseRefreshTokenLines } from "@/lib/refresh-tokens";
import { supplierChannel } from "@/components/providers";
import { toast } from "sonner";

import { useQueryId, useResource } from "@/lib/hooks";
import { usePreference, useSavedFilters, validView } from "@/lib/preferences";
import { useIsMobile } from "@/hooks/use-mobile";

const validSupplierFilters = (value: { status: string; provider_id: string }) =>
  ["", "active", "disabled", "error", "payment_required", "quota_exhausted"].includes(
    value.status,
  ) && ["", "chatgpt", "grok"].includes(value.provider_id);

const untaggedFilter = "__untagged__";

export function SuppliersPage() {
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

  const supplierActions = (item: Supplier) => (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button type="button" variant="ghost" size="icon-sm" aria-label="更多操作">
          <MoreHorizontal />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem asChild>
          <Link href={`/suppliers/detail/?id=${encodeURIComponent(item.id)}`}>
            <ArrowRight /> 详情
          </Link>
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          disabled={!resource.ready || actions.isBusy("reset-state-" + item.id)}
          onSelect={() =>
            void actions.run(
              "reset-state-" + item.id,
              async () => {
                await request(`/suppliers/${item.id}/reset-state`, { method: "POST" });
                resource.reload();
              },
              { success: "状态已重置" },
            )
          }
        >
          重置状态
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={!resource.ready || actions.isBusy("supplier-status-" + item.id)}
          onSelect={() =>
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
            )
          }
        >
          {item.enabled ? "停用账户" : "启用账户"}
        </DropdownMenuItem>
        <DropdownMenuItem
          variant="destructive"
          disabled={!resource.ready || actions.isBusy("supplier-delete-" + item.id)}
          onSelect={() =>
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
            )
          }
        >
          删除账户
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
  useErrorToast(resource.error);
  return (
    <>
      <Card>
        <CardContent className="space-y-3">
          <div className="flex flex-wrap items-end gap-3">
            <form
              className="grid w-full grid-cols-2 items-end gap-3 sm:flex sm:w-auto sm:flex-wrap"
              onSubmit={(event) => {
                event.preventDefault();
                setApplied({ ...filters });
                clearSelection();
                resource.reload(1);
              }}
            >
              <Field className="min-w-0 sm:w-40">
                <FieldLabel htmlFor={`${fieldId}-search`}>搜索账户</FieldLabel>
                <Input
                  id={`${fieldId}-search`}
                  value={filters.search}
                  placeholder="名称或邮箱"
                  onChange={(event) => setFilters({ ...filters, search: event.target.value })}
                />
              </Field>
              <Field className="min-w-0 sm:w-40">
                <FieldLabel htmlFor={`${fieldId}-platform`}>平台</FieldLabel>
                <Select
                  value={filters.provider_id || "all"}
                  onValueChange={(value) =>
                    setFilters({ ...filters, provider_id: value === "all" ? "" : value, tag: "" })
                  }
                >
                  <SelectTrigger id={`${fieldId}-platform`} className="w-full">
                    <SelectValue placeholder="全部平台" />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="all">全部平台</SelectItem>
                    <SelectItem value="chatgpt">ChatGPT</SelectItem>
                    <SelectItem value="grok">Grok</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
              <Field className="min-w-0 sm:w-40">
                <FieldLabel htmlFor={`${fieldId}-status`}>状态</FieldLabel>
                <Select
                  value={filters.status || "all"}
                  onValueChange={(value) =>
                    setFilters({ ...filters, status: value === "all" ? "" : value })
                  }
                >
                  <SelectTrigger id={`${fieldId}-status`} className="w-full">
                    <SelectValue placeholder="全部状态" />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="all">全部状态</SelectItem>
                    {[
                      { value: "active", label: "启用" },
                      { value: "disabled", label: "停用" },
                      { value: "error", label: "授权失效" },
                      { value: "payment_required", label: "账单受限" },
                      { value: "quota_exhausted", label: "配额耗尽" },
                    ].map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              <Field className="min-w-0 sm:w-40">
                <FieldLabel htmlFor={`${fieldId}-tag`}>标签</FieldLabel>
                <Select
                  value={filters.tag || "all"}
                  onValueChange={(value) =>
                    setFilters({ ...filters, tag: value === "all" ? "" : value })
                  }
                >
                  <SelectTrigger id={`${fieldId}-tag`} className="w-full">
                    <SelectValue placeholder="全部标签" />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="all">全部标签</SelectItem>
                    <SelectItem value={untaggedFilter}>无标签</SelectItem>
                    {tagOptions.map((tag) => (
                      <SelectItem key={tag.id} value={tag.id}>
                        {tag.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              <div className="col-span-2 flex items-center gap-2">
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
                    clearSelection();
                    resource.reload(1);
                  }}
                >
                  <RotateCcw />
                  重置
                </Button>
              </div>
            </form>
            <div className="ml-auto flex items-center gap-2">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="显示列"
                    title="显示列"
                  >
                    <Columns3 />
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
                      onCheckedChange={(checked) =>
                        tableColumns0.setVisible(label, checked === true)
                      }
                    >
                      {label}
                    </DropdownMenuCheckboxItem>
                  ))}
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onSelect={tableColumns0.showAll}>显示全部列</DropdownMenuItem>
                  <DropdownMenuItem onSelect={tableColumns0.reset}>恢复默认列</DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                className="hidden md:inline-flex"
                aria-label={view === "table" ? "切换为卡片视图" : "切换为表格视图"}
                title={view === "table" ? "切换为卡片视图" : "切换为表格视图"}
                onClick={() => setView(view === "table" ? "cards" : "table")}
              >
                {view === "table" ? <LayoutGrid /> : <Table2 />}
              </Button>
              <Button type="button" onClick={() => setAdd(true)}>
                <Plus />
                添加供应账户
              </Button>
            </div>
          </div>
          {selectedAccounts.length > 0 && (
            <div
              role="toolbar"
              aria-label="已选账户操作"
              className="flex flex-wrap items-center gap-2 border-t pt-3"
            >
              <Badge variant="secondary">已选 {selectedAccounts.length} 个账户</Badge>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={!resource.ready || batchBusy || !selectedProvider}
                onClick={() => {
                  setBatchAccounts(selectedAccounts);
                  setBatchOpen(true);
                }}
              >
                更新标签
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={
                  !resource.ready || batchBusy || selectedAccounts.length === resource.data?.total
                }
                onClick={() =>
                  void actions.run("supplier-select", async () => {
                    const version = ++selectionVersion.current;
                    const result = await request<List<SupplierSelection>>(
                      `/suppliers/selection${query(applied)}`,
                    );
                    if (version === selectionVersion.current) setSelected(result.items);
                  })
                }
              >
                全选筛选结果（{resource.data?.total ?? "—"}）
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={batchBusy}
                onClick={() => clearSelection()}
              >
                清除选择
              </Button>
              {!selectedProvider && (
                <span className="text-xs text-muted-foreground">标签操作仅支持同平台账户</span>
              )}
            </div>
          )}
        </CardContent>
      </Card>
      {mobile || view === "table" ? (
        <Card>
          <CardContent>
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
                  <TableHead className="w-9" scope="col">
                    <Checkbox
                      aria-label="选择当前页供应账户"
                      checked={allPageSelected ? true : somePageSelected ? "indeterminate" : false}
                      disabled={!resource.ready || batchBusy || !pageIds.length}
                      onCheckedChange={(checked) => toggleSelection(pageIds, checked === true)}
                    />
                  </TableHead>
                  {tableColumns0.labels.map((label) => (
                    <TableHead
                      hidden={!tableColumns0.isVisible(label)}
                      className={
                        ["供应账户", "状态", "操作"].includes(label)
                          ? label === "操作"
                            ? "w-12"
                            : label === "供应账户"
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
                    {pagination.rows.map((item) => (
                      <TableRow
                        role="row"
                        key={item.id}
                        data-state={selectedIds.has(item.id) ? "selected" : undefined}
                      >
                        <TableCell role="cell" className="w-9" data-label="选择">
                          <Checkbox
                            aria-label={`选择供应账户 ${item.email || item.display_name || item.id}`}
                            checked={selectedIds.has(item.id)}
                            disabled={!resource.ready || batchBusy}
                            onCheckedChange={(checked) =>
                              toggleSelection([item.id], checked === true)
                            }
                          />
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("供应账户")}
                          className=" max-md:overflow-hidden"
                          data-label="供应账户"
                          role="cell"
                        >
                          <div className="max-md:hidden">
                            <Link href={`/suppliers/detail/?id=${encodeURIComponent(item.id)}`}>
                              <strong>{item.email || item.display_name || item.id}</strong>
                            </Link>
                          </div>
                          <Dialog>
                            <DialogTrigger asChild>
                              <Button
                                type="button"
                                variant="ghost"
                                className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                                aria-label={
                                  "查看详情：" + String(item.email || item.display_name || item.id)
                                }
                              >
                                <span className="min-w-0 flex-1">
                                  <span className="block truncate font-medium">
                                    {item.email || item.display_name || item.id}
                                  </span>
                                  <span className="block truncate text-xs text-muted-foreground">
                                    {subscriptionLabel(item.plan_type, item.provider_id)}
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
                                  <FieldTitle>供应账户</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    <Link
                                      href={`/suppliers/detail/?id=${encodeURIComponent(item.id)}`}
                                    >
                                      <strong>{item.email || item.display_name || item.id}</strong>
                                    </Link>
                                    <CardDescription>{item.email}</CardDescription>
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>提供商 / 订阅</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    {item.provider_id}
                                    <CardDescription>
                                      {subscriptionLabel(item.plan_type, item.provider_id)}
                                    </CardDescription>
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>套餐到期</FieldTitle>
                                  <FieldDescription>
                                    {date(item.subscription_expires_at)}
                                  </FieldDescription>
                                </Field>
                                <Field>
                                  <FieldTitle>状态</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    <Badge
                                      title={item.error_message ?? undefined}
                                      variant={
                                        item.status === "error"
                                          ? "destructive"
                                          : item.status === "active"
                                            ? "secondary"
                                            : "outline"
                                      }
                                    >
                                      {supplierStatusLabel(item.status)}
                                    </Badge>
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>额度</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    <div
                                      className="flex w-80 flex-wrap gap-2"
                                      aria-label="官方额度"
                                      title={
                                        item.quota
                                          ? "缓存于 " + date(item.quota.observed_at)
                                          : "暂无额度缓存"
                                      }
                                    >
                                      {item.quota?.windows?.map((window) => (
                                        <div
                                          key={window.id}
                                          className="min-w-0 flex-1 basis-64 space-y-1 text-xs"
                                        >
                                          <div className="flex items-center justify-between gap-2">
                                            <span
                                              className="truncate"
                                              title={quotaResetLabel(window.reset_at, now)}
                                            >
                                              {quotaWindowLabel(window)}：
                                              {quotaResetLabel(window.reset_at, now)}
                                            </span>
                                            <span
                                              className="shrink-0 tabular-nums"
                                              title={cycleUsageTitle(window)}
                                            >
                                              {cycleUsageLabel(window)}
                                            </span>
                                          </div>
                                          <div className="flex items-center gap-2">
                                            {window?.used_percent != null ? (
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
                                              <span className="text-muted-foreground">
                                                额度未提供
                                              </span>
                                            )}
                                          </div>
                                        </div>
                                      ))}
                                      {!item.quota?.windows?.length && (
                                        <span className="text-xs text-muted-foreground">
                                          {item.quota?.windows
                                            ? "官方未提供额度窗口"
                                            : item.quota
                                              ? "额度数据未加载"
                                              : "暂无额度缓存"}
                                        </span>
                                      )}
                                    </div>
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>标签</FieldTitle>
                                  <div className="flex flex-wrap items-center gap-1">
                                    {(item.tag_ids ?? []).map((id) => (
                                      <Badge variant="secondary" key={id}>
                                        {item.tags?.find((tag) => tag.id === id)?.name ?? "标签"}
                                      </Badge>
                                    ))}
                                    {!item.tag_ids?.length && (
                                      <span className="text-muted-foreground">—</span>
                                    )}
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>绑定数</FieldTitle>
                                  <FieldDescription>{item.binding_count ?? 0}</FieldDescription>
                                </Field>
                                <Field>
                                  <FieldTitle>最近使用</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    {date(item.last_used_at)}
                                  </div>
                                </Field>
                              </FieldGroup>
                            </DialogContent>
                          </Dialog>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("提供商 / 订阅")}
                          className=" "
                          data-label="提供商 / 订阅"
                          role="cell"
                        >
                          {item.provider_id}
                          <CardDescription>
                            {subscriptionLabel(item.plan_type, item.provider_id)}
                          </CardDescription>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("套餐到期")}
                          data-label="套餐到期"
                          role="cell"
                        >
                          {date(item.subscription_expires_at)}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("状态")}
                          className=" max-md:overflow-hidden"
                          data-label="状态"
                          data-compact="true"
                          role="cell"
                        >
                          <Badge
                            title={item.error_message ?? undefined}
                            variant={
                              item.status === "error"
                                ? "destructive"
                                : item.status === "active"
                                  ? "secondary"
                                  : "outline"
                            }
                          >
                            {supplierStatusLabel(item.status)}
                          </Badge>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("额度")}
                          className=" "
                          data-label="额度"
                          role="cell"
                        >
                          <div
                            className="flex w-80 flex-wrap gap-2"
                            aria-label="官方额度"
                            title={
                              item.quota ? "缓存于 " + date(item.quota.observed_at) : "暂无额度缓存"
                            }
                          >
                            {item.quota?.windows?.map((window) => (
                              <div
                                key={window.id}
                                className="min-w-0 flex-1 basis-64 space-y-1 text-xs"
                              >
                                <div className="flex items-center justify-between gap-2">
                                  <span
                                    className="truncate"
                                    title={quotaResetLabel(window.reset_at, now)}
                                  >
                                    {quotaWindowLabel(window)}：
                                    {quotaResetLabel(window.reset_at, now)}
                                  </span>
                                  <span
                                    className="shrink-0 tabular-nums"
                                    title={cycleUsageTitle(window)}
                                  >
                                    {cycleUsageLabel(window)}
                                  </span>
                                </div>
                                <div className="flex items-center gap-2">
                                  {window?.used_percent != null ? (
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
                                    <span className="text-muted-foreground">额度未提供</span>
                                  )}
                                </div>
                              </div>
                            ))}
                            {!item.quota?.windows?.length && (
                              <span className="text-xs text-muted-foreground">
                                {item.quota?.windows
                                  ? "官方未提供额度窗口"
                                  : item.quota
                                    ? "额度数据未加载"
                                    : "暂无额度缓存"}
                              </span>
                            )}
                          </div>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("标签")}
                          data-label="标签"
                          role="cell"
                        >
                          <div className="flex items-center gap-1">
                            {(item.tag_ids ?? []).slice(0, 2).map((id) => (
                              <Badge variant="secondary" className="max-w-32" key={id}>
                                <span className="truncate">
                                  {item.tags?.find((tag) => tag.id === id)?.name ?? "标签"}
                                </span>
                              </Badge>
                            ))}
                            {(item.tag_ids?.length ?? 0) > 2 && (
                              <Popover>
                                <PopoverTrigger asChild>
                                  <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label="查看全部标签"
                                  >
                                    <Plus />
                                  </Button>
                                </PopoverTrigger>
                                <PopoverContent
                                  align="start"
                                  aria-label="全部标签"
                                  className="max-h-64 overflow-y-auto"
                                >
                                  <div className="flex flex-wrap gap-1">
                                    {(item.tag_ids ?? []).map((id) => (
                                      <Badge
                                        variant="secondary"
                                        className="h-auto max-w-full whitespace-normal break-all"
                                        key={id}
                                      >
                                        {item.tags?.find((tag) => tag.id === id)?.name ?? "标签"}
                                      </Badge>
                                    ))}
                                  </div>
                                </PopoverContent>
                              </Popover>
                            )}
                            {!item.tag_ids?.length && (
                              <span className="text-muted-foreground">—</span>
                            )}
                          </div>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("绑定数")}
                          className="tabular-nums"
                          data-label="绑定数"
                          role="cell"
                        >
                          {item.binding_count ?? 0}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("最近使用")}
                          className=" "
                          data-label="最近使用"
                          role="cell"
                        >
                          {date(item.last_used_at)}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("操作")}
                          className="w-12"
                          data-label="操作"
                          role="cell"
                        >
                          {supplierActions(item)}
                        </TableCell>
                      </TableRow>
                    ))}
                  </>
                ) : (
                  <TableRow role="row">
                    <TableCell role="cell" colSpan={tableColumns0.count + 1}>
                      <Empty>
                        <EmptyDescription>{"暂无符合条件的供应账户"}</EmptyDescription>
                      </Empty>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      ) : items.length ? (
        <div className="grid items-start gap-3 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
          {pagination.rows.map((item) => (
            <Card key={item.id} size="sm">
              <CardContent className="space-y-3">
                <div className="flex items-start justify-between gap-2">
                  <Checkbox
                    aria-label={`选择供应账户 ${item.email || item.display_name || item.id}`}
                    checked={selectedIds.has(item.id)}
                    disabled={!resource.ready || batchBusy}
                    onCheckedChange={(checked) => toggleSelection([item.id], checked === true)}
                  />
                  <div className="min-w-0 flex-1 break-words">
                    <Link href={`/suppliers/detail/?id=${encodeURIComponent(item.id)}`}>
                      <strong>{item.email || item.display_name || item.id}</strong>
                    </Link>
                    <CardDescription>{item.email || "未提供邮箱"}</CardDescription>
                  </div>
                  <Badge
                    className="shrink-0"
                    title={item.error_message ?? undefined}
                    variant={
                      item.status === "error"
                        ? "destructive"
                        : item.status === "active"
                          ? "secondary"
                          : "outline"
                    }
                  >
                    {supplierStatusLabel(item.status)}
                  </Badge>
                </div>
                <FieldGroup className="grid grid-cols-2 gap-x-3 gap-y-2">
                  {[
                    { label: "提供商", value: item.provider_id },
                    {
                      label: "上游订阅",
                      value: subscriptionLabel(item.plan_type, item.provider_id),
                    },
                    { label: "套餐到期", value: date(item.subscription_expires_at) },
                    { label: "最近使用", value: date(item.last_used_at) },
                    { label: "绑定数", value: item.binding_count ?? 0 },
                  ].map(({ label, value }) => (
                    <Field
                      key={label}
                      orientation="horizontal"
                      className={
                        ["套餐到期", "最近使用"].includes(label) ? "col-span-2 min-w-0" : "min-w-0"
                      }
                    >
                      <FieldTitle className="shrink-0">{label}</FieldTitle>
                      <FieldDescription className="min-w-0 break-words">
                        {value ?? "—"}
                      </FieldDescription>
                    </Field>
                  ))}
                  <Field orientation="horizontal" className="col-span-2 min-w-0">
                    <FieldTitle className="shrink-0">标签</FieldTitle>
                    <div className="flex min-w-0 flex-wrap items-center gap-1">
                      {(item.tag_ids ?? []).slice(0, 2).map((id) => (
                        <Badge variant="secondary" className="max-w-32" key={id}>
                          <span className="truncate">
                            {item.tags?.find((tag) => tag.id === id)?.name ?? "标签"}
                          </span>
                        </Badge>
                      ))}
                      {(item.tag_ids?.length ?? 0) > 2 && (
                        <Popover>
                          <PopoverTrigger asChild>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon-xs"
                              aria-label="查看全部标签"
                            >
                              <Plus />
                            </Button>
                          </PopoverTrigger>
                          <PopoverContent
                            align="start"
                            aria-label="全部标签"
                            className="max-h-64 overflow-y-auto"
                          >
                            <div className="flex flex-wrap gap-1">
                              {(item.tag_ids ?? []).map((id) => (
                                <Badge
                                  variant="secondary"
                                  className="h-auto max-w-full whitespace-normal break-all"
                                  key={id}
                                >
                                  {item.tags?.find((tag) => tag.id === id)?.name ?? "标签"}
                                </Badge>
                              ))}
                            </div>
                          </PopoverContent>
                        </Popover>
                      )}
                      {!item.tag_ids?.length && <span className="text-muted-foreground">—</span>}
                    </div>
                  </Field>
                </FieldGroup>
                <div
                  className="flex w-full flex-wrap gap-2"
                  aria-label="官方额度"
                  title={item.quota ? "缓存于 " + date(item.quota.observed_at) : "暂无额度缓存"}
                >
                  {item.quota?.windows?.map((window) => (
                    <div key={window.id} className="min-w-0 flex-1 basis-64 space-y-1 text-xs">
                      <div className="flex items-center justify-between gap-2">
                        <span className="truncate" title={quotaResetLabel(window.reset_at, now)}>
                          {quotaWindowLabel(window)}：{quotaResetLabel(window.reset_at, now)}
                        </span>
                        <span className="shrink-0 tabular-nums" title={cycleUsageTitle(window)}>
                          {cycleUsageLabel(window)}
                        </span>
                      </div>
                      <div className="flex items-center gap-2">
                        {window?.used_percent != null ? (
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
                          <span className="text-muted-foreground">额度未提供</span>
                        )}
                      </div>
                    </div>
                  ))}
                  {!item.quota?.windows?.length && (
                    <span className="text-xs text-muted-foreground">
                      {item.quota?.windows
                        ? "官方未提供额度窗口"
                        : item.quota
                          ? "额度数据未加载"
                          : "暂无额度缓存"}
                    </span>
                  )}
                </div>
                <div className="border-t pt-2">{supplierActions(item)}</div>
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
            <EmptyDescription>暂无符合条件的供应账户</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
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
      <SupplierTagsBatchDialog
        open={batchOpen}
        onOpenChange={setBatchOpen}
        accounts={batchAccounts}
        disabled={!resource.ready}
        onSaved={() => {
          setBatchOpen(false);
          clearSelection();
          resource.reload();
          tags.reload();
        }}
      />
      {add && (
        <OAuthWizard
          onClose={() => setAdd(false)}
          onComplete={() => {
            setAdd(false);
            resource.reload();
          }}
        />
      )}
    </>
  );
}
export function SupplierDetail() {
  const actions = useActions();
  const id = useQueryId();
  const resource = useResource<Supplier>(id ? `/suppliers/${encodeURIComponent(id)}` : null);
  const [tab, setTab] = useState("info");
  const [relogin, setRelogin] = useState(false);
  useErrorToast(id === "" ? "缺少供应账户编号。" : undefined);
  useErrorToast(resource.error ? resource.error : undefined);
  if (id === "")
    return (
      <Empty>
        <EmptyDescription>请选择供应账户</EmptyDescription>
        <Button asChild variant="outline">
          <Link href="/suppliers/">返回列表</Link>
        </Button>
      </Empty>
    );

  const account = resource.data;
  const ChannelOfficialData = supplierChannel(account?.provider_id).OfficialData;
  const ChannelModels = supplierChannel(account?.provider_id).Models;
  const profileRefresh = supplierChannel(account?.provider_id).profileRefresh;
  return (
    <>
      <div className="flex items-center justify-end gap-2">
        <CardDescription className="mr-auto truncate">{account?.email ?? "—"}</CardDescription>
        {profileRefresh && (
          <Button
            variant="outline"
            size="sm"
            disabled={!resource.ready || actions.isBusy("supplier-profile")}
            onClick={() =>
              void actions.run("supplier-profile", async () => {
                await request(profileRefresh(id), { method: "POST" });
                resource.reload();
              })
            }
          >
            刷新官方资料
          </Button>
        )}

        <Button
          variant="outline"
          size="sm"
          onClick={resource.reload}
          disabled={resource.refreshing}
        >
          {resource.refreshing && <Spinner />}刷新
        </Button>
        <Button variant="outline" asChild>
          <Link href="/suppliers/">返回列表</Link>
        </Button>
      </div>
      <Tabs value={tab} onValueChange={setTab} className="min-w-0 gap-4">
        <div className="max-w-full overflow-x-auto overflow-y-hidden pb-1">
          <TabsList variant="line" aria-label={"供应账户详情"}>
            {[
              ["info", "账户资料"],
              ["tags", "标签"],
              ["fingerprint", "指纹与网络"],
              ["quota", "官方额度"],
              ...(ChannelModels ? [["models", "官方模型"]] : []),
              ["local-usage", "本地用量"],
              ["usage", supplierChannel(account?.provider_id).settingsLabel],
              ["details", "官方资料"],
              ["credits", supplierChannel(account?.provider_id).creditsLabel],
            ].map(([key, text]) => (
              <TabsTrigger key={key} value={key}>
                {text}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        <TabsContent value={tab} className="space-y-4">
          {tab === "info" && (
            <Card>
              <CardHeader>
                <CardTitle role="heading" aria-level={2}>
                  {"供应账户资料"}
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <FieldGroup className="grid gap-4 sm:grid-cols-2 gap-3">
                  {[
                    { label: "提供商", value: account?.provider_id },
                    { label: "邮箱", value: account?.email },
                    {
                      label: "状态",
                      value: (
                        <Badge
                          variant={
                            account?.status === "error"
                              ? "destructive"
                              : account?.status === "active"
                                ? "secondary"
                                : "outline"
                          }
                        >
                          {account ? supplierStatusLabel(account.status) : "—"}
                        </Badge>
                      ),
                    },
                    {
                      label: "上游订阅",
                      value: subscriptionLabel(account?.plan_type, account?.provider_id),
                    },
                    { label: "套餐到期", value: date(account?.subscription_expires_at) },
                    { label: "上游空间编号", value: account?.chatgpt_account_id },
                    { label: "上游用户编号", value: account?.chatgpt_user_id },
                    { label: "创建时间", value: date(account?.created_at) },
                    { label: "最近使用", value: date(account?.last_used_at) },
                  ].map(({ label, value }) => (
                    <Field key={label}>
                      <FieldTitle>{label}</FieldTitle>
                      <FieldDescription>{value ?? "—"}</FieldDescription>
                    </Field>
                  ))}
                </FieldGroup>
                <div className="mt-4 flex flex-wrap items-center gap-2">
                  <Button disabled={!resource.ready} onClick={() => setRelogin(true)}>
                    重新授权
                  </Button>
                  <Button
                    type="button"
                    variant={false ? "destructive" : "outline"}
                    disabled={
                      !resource.ready || actions.isBusy("components\\suppliers.tsx:action:5")
                    }
                    onClick={() =>
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
                      )
                    }
                  >
                    {account?.enabled ? "停用" : "启用"}
                  </Button>
                </div>
              </CardContent>
            </Card>
          )}
          {tab === "tags" && (
            <Card>
              <CardContent>
                <SupplierTagEditor
                  key={id}
                  accounts={account ? [account] : []}
                  disabled={!resource.ready}
                  onSaved={resource.reload}
                />
              </CardContent>
            </Card>
          )}
          {tab === "fingerprint" && (
            <FingerprintEditor
              key={id}
              account={account}
              disabled={!resource.ready}
              onSaved={resource.reload}
            />
          )}
          {tab === "local-usage" && <LocalUsage account={account} />}
          {tab === "models" && ChannelModels && <ChannelModels id={id} />}
          {["quota", "usage", "details", "credits"].includes(tab) && (
            <ChannelOfficialData key={tab} id={id} section={tab} onUpdated={resource.reload} />
          )}
        </TabsContent>
      </Tabs>
      {relogin && (
        <OAuthWizard
          supplierId={id}
          supplierProvider={account?.provider_id}
          onClose={() => setRelogin(false)}
          onComplete={() => {
            setRelogin(false);
            resource.reload();
          }}
        />
      )}
    </>
  );
}
function LocalUsage({ account }: { account?: Supplier }) {
  const now = useQuotaClock();
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          本地用量
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <FieldGroup className="grid gap-3 sm:grid-cols-2" aria-label="周期使用额度">
          {account?.quota?.windows.map((window) => (
            <Field key={window.id}>
              <FieldTitle>{quotaWindowLabel(window)}周期已用</FieldTitle>
              <FieldDescription className="tabular-nums" title={cycleUsageTitle(window)}>
                {cycleUsageLabel(window)}
              </FieldDescription>
              <FieldDescription>{quotaResetLabel(window.reset_at, now)}</FieldDescription>
            </Field>
          ))}
          {!account?.quota?.windows.length && (
            <FieldDescription>暂无官方额度周期，暂不能统计周期用量。</FieldDescription>
          )}
        </FieldGroup>
        <FieldGroup className="grid gap-3 sm:grid-cols-2">
          <Field>
            <FieldTitle>累计 Token</FieldTitle>
            <FieldDescription>{tokenCount(account?.usage?.lifetime_tokens)}</FieldDescription>
          </Field>
          <Field>
            <FieldTitle>单日峰值 Token</FieldTitle>
            <FieldDescription>{tokenCount(account?.usage?.peak_daily_tokens)}</FieldDescription>
          </Field>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
function FingerprintEditor({
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
  const ChannelFingerprintFields = supplierChannel(account?.provider_id).FingerprintFields;
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {"指纹与出站网络"}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <FieldGroup className="grid gap-4 sm:grid-cols-2 gap-3">
          {[
            { label: "Originator", value: account?.originator },
            { label: "安装标识", value: account?.installation_id },
            { label: "User-Agent", value: account?.user_agent },
          ].map(({ label, value }) => (
            <Field key={label}>
              <FieldTitle>{label}</FieldTitle>
              <FieldDescription>{value ?? "—"}</FieldDescription>
            </Field>
          ))}
        </FieldGroup>
        <Separator />
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          aria-busy={actions.isBusy("components\\suppliers.tsx:form:12")}
          onSubmit={(event) =>
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
            )
          }
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={disabled || actions.isBusy("components\\suppliers.tsx:form:12")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      账户指纹
                    </CardTitle>
                  </div>
                  <ChannelFingerprintFields
                    value={value}
                    onChange={setChanges}
                    proxies={proxies.data?.items ?? []}
                  />
                </section>
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
            <Button
              type="submit"
              disabled={disabled || actions.isBusy("components\\suppliers.tsx:form:12")}
            >
              {actions.isBusy("components\\suppliers.tsx:form:12") && <Spinner />}
              {actions.isBusy("components\\suppliers.tsx:form:12") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  );
}
export function OAuthWizard({
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
  const ChannelFingerprintFields = channel.FingerprintFields;
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
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent
        {...dialogFocus}
        showCloseButton={false}
        className="flex max-h-[90dvh] min-h-0 flex-col sm:max-w-3xl"
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (busy) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (busy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{supplierId ? "重新授权供应账户" : "添加供应账户"}</DialogTitle>
        </DialogHeader>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          className="absolute right-4 top-4"
          aria-label="关闭"
          disabled={busy}
          onClick={close}
        >
          <X />
        </Button>
        <Tabs
          value={String(step)}
          onValueChange={(next) => back(Number(next))}
          className="min-h-0 gap-4"
        >
          <TabsList variant="line" className="h-12 w-full shrink-0" aria-label="添加账户步骤">
            {steps.map((item, index) => (
              <TabsTrigger
                key={item.value}
                value={String(item.value)}
                disabled={busy || item.value > step}
                aria-current={item.value === step ? "step" : undefined}
              >
                <Badge variant="outline" className="size-5 justify-center rounded-full p-0">
                  {index + 1}
                </Badge>
                {item.label}
              </TabsTrigger>
            ))}
          </TabsList>
          <form
            noValidate
            className="flex min-h-0 flex-col gap-4"
            aria-busy={busy}
            onSubmit={(event) => {
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
            }}
          >
            <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-14rem)]">
              <TabsContent value="0" className="space-y-3 pr-1">
                <Field>
                  <FieldLabel htmlFor={`${fieldId}-provider`}>提供商</FieldLabel>
                  <Select
                    value={provider}
                    disabled={busy}
                    onValueChange={(value) => {
                      setProvider(value);
                      setFingerprint(undefined);
                      setBatchResults([]);
                    }}
                  >
                    <SelectTrigger id={`${fieldId}-provider`}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="chatgpt">ChatGPT</SelectItem>
                      <SelectItem value="grok">Grok</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                {(!setup.ready || !proxies.ready) && (setup.error || proxies.error) && (
                  <Button
                    type="button"
                    variant="outline"
                    disabled={busy}
                    onClick={() => {
                      setup.reload();
                      proxies.reload();
                    }}
                  >
                    重新加载授权配置
                  </Button>
                )}
                <FieldSet disabled={!ready || busy}>
                  <ChannelFingerprintFields
                    value={
                      fingerprint ??
                      setup.data?.fingerprint ?? {
                        os_type: "",
                        os_version: "",
                        arch: "",
                        terminal: "",
                        proxy_id: null,
                        timezone: "",
                      }
                    }
                    onChange={setFingerprint}
                    proxies={proxies.data?.items ?? []}
                  />
                </FieldSet>
              </TabsContent>
              <TabsContent value="1" className="pr-1">
                <RadioGroup
                  value={method}
                  disabled={busy}
                  aria-label="授权方式"
                  onValueChange={(next) => {
                    setMethod(next as typeof method);
                    setRefreshToken("");
                    setBatchResults([]);
                  }}
                >
                  {methods.map((option) => (
                    <FieldLabel key={option.value} htmlFor={`${fieldId}-${option.value}`}>
                      <Field orientation="horizontal">
                        <RadioGroupItem
                          value={option.value}
                          id={`${fieldId}-${option.value}`}
                          aria-label={option.label}
                        />
                        <FieldContent>
                          <FieldTitle>{option.label}</FieldTitle>
                          <FieldDescription>{option.description}</FieldDescription>
                        </FieldContent>
                      </Field>
                    </FieldLabel>
                  ))}
                </RadioGroup>
              </TabsContent>
              <TabsContent value="2" className="space-y-4 pr-1">
                <FieldSet disabled={busy} className="gap-4">
                  {method === "refresh_token" ? (
                    <Field>
                      <FieldLabel htmlFor={`${fieldId}-refresh-token`}>Refresh Token</FieldLabel>
                      <Textarea
                        id={`${fieldId}-refresh-token`}
                        autoComplete="off"
                        spellCheck={false}
                        rows={supplierId ? 2 : 6}
                        required
                        value={refreshToken}
                        onChange={(event) => {
                          setRefreshToken(event.target.value);
                          setBatchResults([]);
                        }}
                      />
                      <FieldDescription>
                        {supplierId ? "输入新的 RT" : "每行一条，最多 50 条"}
                      </FieldDescription>
                    </Field>
                  ) : pending ? (
                    <>
                      {pending.authorize_url && (
                        <Field>
                          <FieldTitle>授权地址</FieldTitle>
                          <div className="flex min-w-0 items-start gap-2">
                            <a
                              id={`${fieldId}-authorize-url`}
                              className="min-w-0 flex-1 break-all text-sm underline underline-offset-4"
                              href={pending.authorize_url}
                              target="_blank"
                              rel="noreferrer"
                            >
                              {pending.authorize_url}
                            </a>
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              disabled={busy || actions.isBusy("copy-supplier-authorize")}
                              onClick={() =>
                                actions.run(
                                  "copy-supplier-authorize",
                                  async () => {
                                    await copyElementText(
                                      document.getElementById(`${fieldId}-authorize-url`),
                                    );
                                  },
                                  { success: "授权地址已复制" },
                                )
                              }
                            >
                              <Copy />
                              复制
                            </Button>
                          </div>
                        </Field>
                      )}
                      {pending.verification_url && (
                        <Button asChild className="w-fit">
                          <a href={pending.verification_url} target="_blank" rel="noreferrer">
                            打开设备验证页面
                          </a>
                        </Button>
                      )}
                      {pending.user_code && (
                        <Field>
                          <FieldTitle>设备验证码</FieldTitle>
                          <FieldDescription className="select-all font-mono text-xl">
                            {pending.user_code}
                          </FieldDescription>
                        </Field>
                      )}
                      {pending.method === "callback" && (
                        <Field>
                          <FieldLabel htmlFor={`${fieldId}-callback`}>
                            {channel.callbackInputLabel}
                          </FieldLabel>
                          <Textarea
                            id={`${fieldId}-callback`}
                            required
                            rows={3}
                            value={callback}
                            onChange={(event) => setCallback(event.target.value)}
                          />
                        </Field>
                      )}
                    </>
                  ) : (
                    <CardDescription>
                      {busy ? "正在准备授权…" : "点击开始授权，获取授权链接。"}
                    </CardDescription>
                  )}
                </FieldSet>
                {batchResults.length > 0 && (
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>行号</TableHead>
                        <TableHead>结果</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {batchResults.map((row) => (
                        <TableRow key={row.line}>
                          <TableCell>{row.line}</TableCell>
                          <TableCell>
                            {row.status === "running" && <Spinner />}
                            {row.supplierId ? (
                              <Link
                                href={`/suppliers/detail/?id=${encodeURIComponent(row.supplierId)}`}
                              >
                                {row.message}
                              </Link>
                            ) : (
                              row.message
                            )}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                )}
              </TabsContent>
            </ScrollArea>
            <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
              {batchResults.some((row) => row.status === "complete") && (
                <Button
                  type="button"
                  variant="outline"
                  disabled={busy}
                  onClick={() => {
                    setRefreshToken("");
                    onComplete();
                  }}
                >
                  完成并刷新列表
                </Button>
              )}
              {step > (supplierId ? 1 : 0) && (
                <Button
                  type="button"
                  variant="outline"
                  disabled={busy}
                  onClick={() => back(step - 1)}
                >
                  上一步
                </Button>
              )}
              <Button type="submit" disabled={!ready || busy}>
                {busy && <Spinner />}
                {step < 2
                  ? "下一步"
                  : busy
                    ? "正在提交…"
                    : pending?.method === "callback"
                      ? "提交回调"
                      : pending?.method === "device"
                        ? "检查授权结果"
                        : "开始授权"}
              </Button>
            </FieldGroup>
          </form>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
