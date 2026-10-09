"use client";
import { OAuthWizard } from "@/app/components/suppliers/oauth-wizard";
import { SupplierRecordDialog } from "@/app/components/suppliers/supplier-record-dialog";
import { SupplierTagsBatchDialog } from "@/app/components/suppliers/supplier-tag-dialog";
import { useSuppliersPage } from "@/app/data/suppliers";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldTitle } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { type Supplier } from "@/lib/api";
import { date } from "@/lib/format";
import { supplierSubscriptionLabel as subscriptionLabel } from "@/lib/subscriptions";
import {
  cycleUsageLabel,
  cycleUsageTitle,
  percentLabel,
  quotaResetLabel,
  quotaWindowLabel,
  supplierStatusLabel,
} from "@/lib/supplier-state";
import {
  ArrowRight,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  Inbox,
  LayoutGrid,
  MoreHorizontal,
  Plus,
  RotateCcw,
  Search,
  Table2,
} from "lucide-react";
import Link from "next/link";

const untaggedFilter = "__untagged__";

export default function SuppliersPage() {
  const {
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
  } = useSuppliersPage();
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
          onSelect={() => handleSelect(item)}
        >
          重置状态
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={!resource.ready || actions.isBusy("supplier-status-" + item.id)}
          onSelect={() => handleSelect2(item)}
        >
          {item.enabled ? "停用账户" : "启用账户"}
        </DropdownMenuItem>
        <DropdownMenuItem
          variant="destructive"
          disabled={!resource.ready || actions.isBusy("supplier-delete-" + item.id)}
          onSelect={() => handleSelect3(item)}
        >
          删除账户
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
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
                onClick={() => handleClick4()}
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
                          <SupplierRecordDialog item={item} now={now} />
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
