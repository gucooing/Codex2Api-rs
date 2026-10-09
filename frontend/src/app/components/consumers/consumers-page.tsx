"use client";
import { ConsumerBatchDialog } from "@/app/components/consumers/consumer-batch-dialog";
import { ConsumerRecordDialog } from "@/app/components/consumers/consumer-record-dialog";
import { CreateConsumerDialog } from "@/app/components/consumers/create-consumer-dialog";
import { useConsumersPage } from "@/app/data/consumers";
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
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { type Consumer } from "@/lib/api";
import { date } from "@/lib/format";
import { subscriptionLabel } from "@/lib/platform-account";
import { percentLabel, quotaResetLabel, quotaWindowLabel } from "@/lib/supplier-state";
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
  RefreshCw,
  RotateCcw,
  Search,
  Table2,
} from "lucide-react";
import Link from "next/link";

export default function ConsumersPage() {
  const {
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
  } = useConsumersPage();
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
                          <ConsumerRecordDialog account={account} now={now} />
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
      <ConsumerBatchDialog
        batchOpen={batchOpen}
        batchBusy={batchBusy}
        setBatchOpen={setBatchOpen}
        dialogFocus={dialogFocus}
        batchDialog={batchDialog}
        actions={actions}
        runBatch={runBatch}
        resource={resource}
        fieldId={fieldId}
        grantQuantity={grantQuantity}
        setGrantQuantity={setGrantQuantity}
        grantStartMode={grantStartMode}
        setGrantStartMode={setGrantStartMode}
        grantActivateAt={grantActivateAt}
        setGrantActivateAt={setGrantActivateAt}
        grantDuration={grantDuration}
        setGrantDuration={setGrantDuration}
        grantNote={grantNote}
        setGrantNote={setGrantNote}
      />
      {create && (
        <CreateConsumerDialog
          actions={actions}
          setCreate={setCreate}
          dialogFocus={dialogFocus}
          resource={resource}
        />
      )}
    </>
  );
}
