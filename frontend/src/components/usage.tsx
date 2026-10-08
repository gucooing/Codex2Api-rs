"use client";
import { usePlatformPrefix } from "@/lib/platform-scope";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuItem,
} from "@/components/ui/dropdown-menu";
import { useColumnVisibility } from "@/lib/columns";

import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import { useId } from "react";
import { Badge } from "@/components/ui/badge";
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
import { Button } from "@/components/ui/button";

import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import {
  ChevronsLeft,
  ChevronLeft,
  ChevronRight,
  ChevronsRight,
  Inbox,
  Columns3,
} from "lucide-react";
import { useErrorToast, validateForm } from "@/lib/actions";
import {
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableBody,
  TableCell,
} from "@/components/ui/table";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";

import { Progress } from "@/components/ui/progress";
import { date, money } from "@/lib/format";
import { useState } from "react";
import { Search, RotateCcw, CalendarDays } from "lucide-react";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  query,
  type UsagePage,
  type UsageRecord,
  type AccountUsage,
  type Supplier,
  type Consumer,
  type List,
} from "@/lib/api";
import { billingLabel } from "@/lib/domain";
import {
  failedUsage,
  tokenCount,
  cacheRate,
  usageFailure,
  usageFailureKind,
  usageResultCode,
  duration,
  usageStatus,
  usageStatuses,
  imageUsageLabel,
  requestSpeed,
} from "@/lib/usage-display";
import { Info } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { FieldGroup, FieldTitle, FieldDescription } from "@/components/ui/field";
import { useResource } from "@/lib/hooks";
import { usePreference, useSavedFilters, validPageSize } from "@/lib/preferences";
import { usePageControls } from "@/lib/pagination";
import { useIsMobile } from "@/hooks/use-mobile";

import { useUserLookup, userOptionLabel, type UserOption } from "@/lib/user-lookup";

const emptyFilters = {
  user_id: "",
  user_label: "",
  supplier_label: "",
  consumer_label: "",
  supplier_id: "",
  virtual_account: "",
  model: "",
  status: "",
  from: "",
  until: "",
};
export function UsagePageView({ consumerId }: { consumerId?: string }) {
  const fieldId = useId();
  const userLookup = useUserLookup();
  const {
    filters,
    setFilters,
    applied,
    setApplied,
    ready: preferencesReady,
  } = useSavedFilters(`usage:${consumerId ?? "all"}:filters`, emptyFilters);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = usePreference<number>("usage.page-size", 20, validPageSize);
  const [supplierOpen, setSupplierOpen] = useState(false);
  const [consumerOpen, setConsumerOpen] = useState(false);
  const [supplierSearch, setSupplierSearch] = useState("");
  const [consumerSearch, setConsumerSearch] = useState("");
  type SupplierOption = Pick<Supplier, "id" | "display_name" | "email">;
  type ConsumerOption = Pick<Consumer, "id" | "username" | "email">;
  const selectedUser = filters.user_id
    ? { id: filters.user_id, username: "", name: filters.user_label }
    : null;
  const selectedSupplier = filters.supplier_id
    ? {
        id: filters.supplier_id,
        display_name: filters.supplier_label || filters.supplier_id,
        email: "",
      }
    : null;
  const selectedConsumer = filters.virtual_account
    ? {
        id: filters.virtual_account,
        username: filters.consumer_label || filters.virtual_account,
        email: "",
      }
    : null;
  const suppliers = useResource<List<Supplier>>(
    supplierOpen ? `/suppliers${query({ search: supplierSearch.trim(), limit: 5 })}` : null,
    supplierSearch.trim() ? 250 : 0,
  );
  const consumers = useResource<List<Consumer>>(
    consumerOpen && !consumerId
      ? `/consumers${query({ search: consumerSearch.trim(), limit: 5 })}`
      : null,
    consumerSearch.trim() ? 250 : 0,
  );
  useErrorToast(suppliers.error);
  useErrorToast(consumers.error);

  const resource = useResource<UsagePage>(
    preferencesReady
      ? `/usage${query({ user_id: consumerId ? "" : applied.user_id, supplier_id: applied.supplier_id, virtual_account: consumerId ?? applied.virtual_account, model: applied.model, status: applied.status, from: applied.from, until: applied.until, page, page_size: pageSize, tz_offset: new Date().getTimezoneOffset() })}`
      : null,
  );
  const pagination = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    setPage,
    pageSize,
    resource.refreshing,
    setPageSize,
  );
  const update = (key: keyof typeof filters, value: string) =>
    setFilters((current) => ({ ...current, [key]: value }));
  useErrorToast(resource.error);
  return (
    <>
      <Card>
        <CardContent className="space-y-4">
          <form
            className="space-y-4"
            noValidate
            onSubmit={(e) => {
              e.preventDefault();
              if (!validateForm(e.currentTarget)) return;
              setApplied(filters);
              resource.reload();
              setPage(1);
            }}
          >
            <Collapsible>
              <div className="flex flex-wrap items-end gap-3">
                {!consumerId && (
                  <Field className="w-40">
                    <FieldLabel className="sr-only" htmlFor={fieldId + "-user"}>
                      用户
                    </FieldLabel>
                    <Combobox<UserOption>
                      items={userLookup.data?.items ?? []}
                      value={selectedUser}
                      onValueChange={(item) => {
                        setFilters((v) => ({
                          ...v,
                          user_id: item?.id ?? "",
                          user_label: item ? userOptionLabel(item) : "",
                        }));
                        userLookup.setSearch("");
                      }}
                      itemToStringLabel={userOptionLabel}
                      itemToStringValue={(item) => item.id}
                      isItemEqualToValue={(item, value) => item.id === value.id}
                      filter={null}
                      open={userLookup.open}
                      onOpenChange={(open, details) => {
                        userLookup.setOpen(open);
                        if (open && details.reason !== "input-change") userLookup.setSearch("");
                      }}
                      onInputValueChange={(text, details) => {
                        if (details.reason === "input-change") {
                          userLookup.setSearch(text);
                          if (!text) {
                            setFilters((v) => ({ ...v, user_id: "", user_label: "" }));
                          }
                        }
                      }}
                    >
                      <ComboboxInput
                        id={fieldId + "-user"}
                        placeholder="搜索选择用户"
                        showClear
                        className="w-full"
                        maxLength={128}
                      />
                      <ComboboxContent>
                        <ComboboxEmpty>
                          {userLookup.loading
                            ? "加载中…"
                            : userLookup.error
                              ? "加载失败"
                              : "没有匹配用户"}
                        </ComboboxEmpty>
                        <ComboboxList aria-busy={userLookup.loading}>
                          {(item: UserOption) => (
                            <ComboboxItem key={item.id} value={item}>
                              {userOptionLabel(item)}
                            </ComboboxItem>
                          )}
                        </ComboboxList>
                      </ComboboxContent>
                    </Combobox>
                  </Field>
                )}
                <Field className="w-40">
                  <FieldLabel htmlFor={fieldId + "-supplier"}>供应账户</FieldLabel>
                  <Combobox<SupplierOption>
                    items={suppliers.data?.items ?? []}
                    value={selectedSupplier ?? null}
                    onValueChange={(item) => {
                      setFilters((current) => ({
                        ...current,
                        supplier_id: item?.id ?? "",
                        supplier_label: item?.display_name || item?.email || item?.id || "",
                      }));
                      setSupplierSearch("");
                    }}
                    itemToStringLabel={(item) => item.display_name || item.email || item.id}
                    itemToStringValue={(item) => item.id}
                    isItemEqualToValue={(item, value) => item.id === value.id}
                    filter={null}
                    open={supplierOpen}
                    onOpenChange={(open, details) => {
                      setSupplierOpen(open);
                      if (open && details.reason !== "input-change") setSupplierSearch("");
                    }}
                    onInputValueChange={(text, details) => {
                      if (details.reason === "input-change") {
                        setSupplierSearch(text);
                        if (!text) {
                          setFilters((current) => ({
                            ...current,
                            supplier_id: "",
                            supplier_label: "",
                          }));
                        }
                      }
                    }}
                  >
                    <ComboboxInput
                      id={fieldId + "-supplier"}
                      aria-label="供应账户"
                      placeholder="全部账户"
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
                        {(item: SupplierOption) => (
                          <ComboboxItem key={item.id} value={item}>
                            <span className="flex min-w-0 flex-col">
                              <span className="truncate">
                                {item.display_name || item.email || item.id}
                              </span>
                              <span className="truncate text-xs text-muted-foreground">
                                {item.email}
                              </span>
                            </span>
                          </ComboboxItem>
                        )}
                      </ComboboxList>
                    </ComboboxContent>
                  </Combobox>
                </Field>
                {!consumerId && (
                  <Field className="w-40">
                    <FieldLabel htmlFor={fieldId + "-consumer"}>消费账户</FieldLabel>
                    <Combobox<ConsumerOption>
                      items={consumers.data?.items ?? []}
                      value={selectedConsumer ?? null}
                      onValueChange={(item) => {
                        setFilters((current) => ({
                          ...current,
                          virtual_account: item?.id ?? "",
                          consumer_label: item?.username ?? "",
                        }));
                        setConsumerSearch("");
                      }}
                      itemToStringLabel={(item) => item.username}
                      itemToStringValue={(item) => item.id}
                      isItemEqualToValue={(item, value) => item.id === value.id}
                      filter={null}
                      open={consumerOpen}
                      onOpenChange={(open, details) => {
                        setConsumerOpen(open);
                        if (open && details.reason !== "input-change") setConsumerSearch("");
                      }}
                      onInputValueChange={(text, details) => {
                        if (details.reason === "input-change") {
                          setConsumerSearch(text);
                          if (!text) {
                            setFilters((current) => ({
                              ...current,
                              virtual_account: "",
                              consumer_label: "",
                            }));
                          }
                        }
                      }}
                    >
                      <ComboboxInput
                        id={fieldId + "-consumer"}
                        aria-label="消费账户"
                        placeholder="全部账户"
                        showClear
                        className="w-full"
                      />
                      <ComboboxContent>
                        <ComboboxEmpty>
                          {consumers.loading
                            ? "正在加载…"
                            : consumers.error
                              ? "加载失败，请重新搜索"
                              : "没有匹配账户"}
                        </ComboboxEmpty>
                        <ComboboxList aria-busy={consumers.loading}>
                          {(item: ConsumerOption) => (
                            <ComboboxItem key={item.id} value={item}>
                              <span className="flex min-w-0 flex-col">
                                <span className="truncate">{item.username}</span>
                                <span className="truncate text-xs text-muted-foreground">
                                  {item.email}
                                </span>
                              </span>
                            </ComboboxItem>
                          )}
                        </ComboboxList>
                      </ComboboxContent>
                    </Combobox>
                  </Field>
                )}
                <Field className="w-40">
                  <FieldLabel
                    htmlFor={fieldId + "-field-3" + "-" + encodeURIComponent(String("模型"))}
                  >
                    {"模型"}
                  </FieldLabel>
                  <Input
                    id={fieldId + "-field-3" + "-" + encodeURIComponent(String("模型"))}
                    aria-label={"模型"}
                    value={filters.model}
                    onChange={(e) => update("model", e.target.value)}
                  />
                </Field>
                <Field className="w-40">
                  <FieldLabel
                    htmlFor={fieldId + "-field-4" + "-" + encodeURIComponent(String("状态"))}
                  >
                    {"状态"}
                  </FieldLabel>
                  <Select
                    value={filters.status}
                    onValueChange={(next) =>
                      ((value) => update("status", value))(
                        next ===
                          fieldId + "-field-4" + "-" + encodeURIComponent(String("状态")) + "-empty"
                          ? ""
                          : next,
                      )
                    }
                  >
                    <SelectTrigger
                      id={fieldId + "-field-4" + "-" + encodeURIComponent("状态")}
                      aria-label="状态"
                      className="w-full"
                    >
                      <SelectValue placeholder="全部状态" />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem
                        value={fieldId + "-field-4" + "-" + encodeURIComponent("状态") + "-empty"}
                      >
                        全部状态
                      </SelectItem>
                      {usageStatuses.map((status) => (
                        <SelectItem key={status.value} value={status.value}>
                          {status.label}
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
                    variant="outline"
                    type="button"
                    onClick={() => {
                      const next = { ...emptyFilters, virtual_account: consumerId ?? "" };
                      setSupplierSearch("");
                      setConsumerSearch("");
                      setFilters(next);
                      setApplied(next);
                      resource.reload();
                      setPage(1);
                    }}
                  >
                    <RotateCcw />
                    重置
                  </Button>
                  <CollapsibleTrigger asChild>
                    <Button type="button" variant="ghost">
                      <CalendarDays />
                      时间范围{filters.from || filters.until ? " · 已设置" : ""}
                    </Button>
                  </CollapsibleTrigger>
                </div>
              </div>
              <CollapsibleContent>
                <div className="mt-3 flex flex-wrap items-end gap-3">
                  <Field className="w-56">
                    <FieldLabel
                      htmlFor={fieldId + "-field-5" + "-" + encodeURIComponent(String("开始时间"))}
                    >
                      {"开始时间"}
                    </FieldLabel>
                    <Input
                      id={fieldId + "-field-5" + "-" + encodeURIComponent(String("开始时间"))}
                      aria-label={"开始时间"}
                      type="datetime-local"
                      value={filters.from}
                      max={filters.until || undefined}
                      onChange={(event) => update("from", event.target.value)}
                    />
                  </Field>
                  <Field className="w-56">
                    <FieldLabel
                      htmlFor={fieldId + "-field-6" + "-" + encodeURIComponent(String("结束时间"))}
                    >
                      {"结束时间"}
                    </FieldLabel>
                    <Input
                      id={fieldId + "-field-6" + "-" + encodeURIComponent(String("结束时间"))}
                      aria-label={"结束时间"}
                      type="datetime-local"
                      value={filters.until}
                      min={filters.from || undefined}
                      onChange={(event) => update("until", event.target.value)}
                    />
                  </Field>
                </div>
              </CollapsibleContent>
            </Collapsible>
          </form>
        </CardContent>
      </Card>

      {
        <>
          <Card>
            <CardContent className="space-y-4">
              <UsageTable records={resource.data?.records ?? []} />
              <Pagination aria-label="记录分页" className="justify-end">
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
        </>
      }
    </>
  );
}
function UsageTable({ records, empty }: { records: UsageRecord[]; empty?: string }) {
  const tableColumns0 = useColumnVisibility(
    "components/usage.tsx:0",
    [
      "消费账户",
      "供应账户",
      "模型 / 接口",
      "推理强度 / 速度",
      "用量",
      "费用",
      "耗时",
      "时间 / 状态",
    ],
    ["模型 / 接口", "用量", "时间 / 状态"],
  );

  const mobile = useIsMobile();
  const [selected, setSelected] = useState<UsageRecord>();
  return (
    <>
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
        </div>
        <Table
          role="table"
          className={
            tableColumns0.count > 4
              ? "[&_td]:py-1.5 max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
              : "[&_td]:py-1.5 max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
          }
        >
          <TableHeader>
            <TableRow role="row">
              {[
                "消费账户",
                "供应账户",
                "模型 / 接口",
                "推理强度 / 速度",
                "用量",
                "费用",
                "耗时",
                "时间 / 状态",
              ].map((label) => (
                <TableHead
                  hidden={!tableColumns0.isVisible(label)}
                  className={
                    ["模型 / 接口", "用量", "时间 / 状态"].includes(label)
                      ? label === "模型 / 接口"
                        ? ""
                        : "max-md:w-20"
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
            {records.length ? (
              records.map((record) => {
                const failed = failedUsage(record);
                const mismatch = Boolean(
                  record.actual_model && record.model && record.actual_model !== record.model,
                );
                const metrics = [
                  {
                    label: "输入",
                    value: tokenCount(record.input_tokens),
                    exact: record.input_tokens,
                  },
                  {
                    label: "输出",
                    value: tokenCount(record.output_tokens),
                    exact: record.output_tokens,
                  },
                  {
                    label: "思考",
                    description: "思考（包含在输出中）",
                    value: tokenCount(record.reasoning_tokens),
                    exact: record.reasoning_tokens,
                  },
                  {
                    label: "缓存读取",
                    value: tokenCount(record.cached_tokens),
                    exact: record.cached_tokens,
                  },
                  {
                    label: "缓存写入",
                    value: tokenCount(record.cache_write_tokens),
                    exact: record.cache_write_tokens,
                  },
                  {
                    label: "缓存率",
                    description: "缓存率（缓存读取 / 总输入）",
                    value: cacheRate(record),
                    exact: undefined,
                  },
                ];
                return (
                  <TableRow role="row" key={record.id}>
                    <TableCell
                      hidden={!tableColumns0.isVisible("消费账户")}
                      className=" "
                      data-label="消费账户"
                      role="cell"
                    >
                      {record.subject_name || record.subject_id}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("供应账户")}
                      className=" "
                      data-label="供应账户"
                      role="cell"
                    >
                      {record.account_name}
                      <CardDescription>{record.provider_id}</CardDescription>
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("模型 / 接口")}
                      className=" max-md:overflow-hidden"
                      data-label="模型 / 接口"
                      role="cell"
                    >
                      {mobile ? (
                        <Button
                          type="button"
                          variant="ghost"
                          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                          aria-label={
                            "查看详情：" +
                            String(record.actual_model ?? record.model ?? "未记录模型")
                          }
                          onClick={() => setSelected(record)}
                        >
                          <span className="min-w-0 flex-1">
                            <span className="block truncate font-medium">
                              {record.actual_model ?? record.model ?? "未记录模型"}
                            </span>
                            <span className="block truncate text-xs text-muted-foreground">
                              {new Date(record.requested_at_ms).toLocaleTimeString("zh-CN", {
                                hour: "2-digit",
                                minute: "2-digit",
                              }) +
                                " · " +
                                (record.subject_name || record.subject_id)}
                            </span>
                          </span>
                          <ChevronRight className="size-3 shrink-0" />
                        </Button>
                      ) : (
                        <div className="max-md:hidden">
                          <strong
                            className={
                              mismatch ? "text-yellow-700 dark:text-yellow-400" : undefined
                            }
                          >
                            {mismatch
                              ? `${record.model} → ${record.actual_model}`
                              : (record.actual_model ?? record.model ?? "-")}
                          </strong>
                          <CardDescription className="whitespace-nowrap text-xs">
                            {record.endpoint} · {record.transport}
                          </CardDescription>
                        </div>
                      )}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("推理强度 / 速度")}
                      data-label="推理强度 / 速度"
                      data-compact="true"
                      role="cell"
                      className="text-xs "
                    >
                      {record.reasoning_effort ?? "默认"} / {requestSpeed(record)}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("用量")}
                      className=" max-md:overflow-hidden"
                      data-label="用量"
                      role="cell"
                    >
                      {mobile ? (
                        <div className="space-y-0.5 text-xs tabular-nums md:hidden">
                          <div>
                            {failed
                              ? "-"
                              : record.image_count !== null
                                ? record.image_count + " 张"
                                : (record.input_tokens == null && record.output_tokens == null
                                    ? "-"
                                    : tokenCount(
                                        (record.input_tokens ?? 0) + (record.output_tokens ?? 0),
                                      )) + " Token"}
                          </div>
                          <div className="text-muted-foreground">
                            {failed || record.cost_nano_usd == null
                              ? "-"
                              : money(record.cost_nano_usd / 1e9)}
                          </div>
                        </div>
                      ) : (
                        <div className="max-md:hidden">
                          {failed ? (
                            "-"
                          ) : record.image_count !== null ? (
                            <span>{imageUsageLabel(record)}</span>
                          ) : (
                            <div className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-xs tabular-nums md:grid-cols-[repeat(3,max-content)]">
                              {metrics.map(({ label, description, value, exact }) => (
                                <Tooltip key={label}>
                                  <TooltipTrigger asChild>
                                    <span
                                      tabIndex={0}
                                      className="inline-flex items-center gap-1 whitespace-nowrap"
                                      aria-label={`${label}：${value}`}
                                    >
                                      <span className="text-muted-foreground">{label}</span>
                                      {value}
                                    </span>
                                  </TooltipTrigger>
                                  <TooltipContent>
                                    {description ?? label}：
                                    {exact == null ? value : exact.toLocaleString("en-US")}
                                  </TooltipContent>
                                </Tooltip>
                              ))}
                            </div>
                          )}
                        </div>
                      )}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("费用")}
                      data-label="费用"
                      data-compact="true"
                      role="cell"
                      className="tabular-nums "
                    >
                      {failed || record.cost_nano_usd === null
                        ? "-"
                        : money(record.cost_nano_usd / 1e9)}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("耗时")}
                      className=" "
                      data-label="耗时"
                      role="cell"
                    >
                      <CardDescription className="text-xs">
                        首字节 {duration(record.first_byte_ms)}
                      </CardDescription>
                      <CardDescription className="text-xs">
                        总计 {duration(record.total_ms)}
                      </CardDescription>
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("时间 / 状态")}
                      className=" max-md:overflow-hidden"
                      data-label="时间 / 状态"
                      role="cell"
                    >
                      <span className="hidden text-xs md:inline">
                        {date(record.requested_at_ms)}
                      </span>
                      <div>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          className="h-6 px-1 tabular-nums"
                          aria-label={`查看请求详情：${usageResultCode(record)}`}
                          onClick={() => setSelected(record)}
                        >
                          <Badge variant="outline" className={usageStatus(record.status).className}>
                            {usageStatus(record.status).label}
                          </Badge>
                          <span className="max-md:hidden">{usageResultCode(record)}</span>
                          <Info className="size-3" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })
            ) : (
              <TableRow role="row">
                <TableCell role="cell" colSpan={tableColumns0.count}>
                  <Empty>
                    <EmptyDescription>{empty ?? "暂无符合条件的用量记录"}</EmptyDescription>
                  </Empty>
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </>
      <Dialog
        open={Boolean(selected)}
        onOpenChange={(open) => {
          if (!open) setSelected(undefined);
        }}
      >
        <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>请求详情</DialogTitle>
            <DialogDescription className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
              <span>{selected ? date(selected.requested_at_ms) : ""}</span>
              <span className="min-w-0 break-all text-xs">
                请求 ID：
                <span className="select-all font-mono">{selected?.upstream_request_id ?? "-"}</span>
              </span>
            </DialogDescription>
          </DialogHeader>
          <FieldGroup className="grid gap-3 sm:grid-cols-2">
            {[
              ["消费账户", selected?.subject_name || selected?.subject_id],
              ["供应账户", selected?.account_name],
              ["请求模型", selected?.model],
              ["实际模型", selected?.actual_model],
              ["HTTP 状态（连接）", selected?.http_status],
              ["生成错误状态", selected?.failure_status],
              ["失败类别", usageFailureKind(selected?.failure_kind)],
              [
                "请求状态",
                selected
                  ? usageStatus(selected.status).label +
                    (selected.status === "client_stopped" ? "（客户端停止）" : "")
                  : "-",
              ],
              ["接口", selected?.endpoint],
              ["传输方式", selected?.transport],
              ["首字节耗时", duration(selected?.first_byte_ms)],
              ["总耗时", duration(selected?.total_ms)],
            ].map(([label, value]) => (
              <Field key={String(label)}>
                <FieldTitle>{label}</FieldTitle>
                <FieldDescription className="break-words">{value ?? "-"}</FieldDescription>
              </Field>
            ))}
          </FieldGroup>
          {selected && failedUsage(selected) && (
            <FieldGroup className="gap-3 border-t pt-3">
              <Field>
                <FieldTitle>失败原因</FieldTitle>
                <FieldDescription className="whitespace-pre-wrap break-words">
                  {usageFailure(selected)}
                </FieldDescription>
              </Field>
              {selected.error_code && (
                <Field>
                  <FieldTitle>错误码</FieldTitle>
                  <FieldDescription>{selected.error_code}</FieldDescription>
                </Field>
              )}
            </FieldGroup>
          )}
          {selected && (
            <FieldGroup className="grid gap-3 border-t pt-3 sm:grid-cols-3">
              {[
                ...(selected.image_count !== null
                  ? [["图片用量", imageUsageLabel(selected)] as const]
                  : []),

                ["输入", selected.input_tokens],
                ["输出", selected.output_tokens],
                ["思考（输出内）", selected.reasoning_tokens],
                ["缓存读取", selected.cached_tokens],
                ["缓存写入", selected.cache_write_tokens],
                ["缓存率", cacheRate(selected)],
                [
                  "费用",
                  selected.cost_nano_usd == null ? "-" : money(selected.cost_nano_usd / 1e9),
                ],
                ["计费状态", billingLabel(selected.billing_status)],

                ["请求速度", requestSpeed(selected)],
                ["命中计费挡位", selected.billing_tier ?? "-"],
              ].map(([label, value]) => (
                <Field key={String(label)}>
                  <FieldTitle>{label}</FieldTitle>
                  <FieldDescription
                    title={typeof value === "number" ? value.toLocaleString("en-US") : undefined}
                  >
                    {typeof value === "number" ? tokenCount(value) : (value ?? "-")}
                  </FieldDescription>
                </Field>
              ))}
            </FieldGroup>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
export function ConsumerUsage({ id }: { id: string }) {
  const platformPrefix = usePlatformPrefix();
  const resource = useResource<AccountUsage>(`${platformPrefix}/${id}/usage`);
  useErrorToast(resource.error);
  return (
    <>
      {
        <div className="grid items-start gap-3 xl:grid-cols-2">
          <Card size="sm">
            <CardHeader>
              <CardTitle role="heading" aria-level={2}>
                {"额度窗口"}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid gap-4 sm:grid-cols-2">
                {[
                  ["主额度", resource.data?.quota.rate_limit.primary_window ?? null],
                  ["次额度", resource.data?.quota.rate_limit.secondary_window ?? null],
                ]
                  .filter(([, window]) => window || !resource.data)
                  .map(([fallback, raw]) => {
                    const window = typeof raw === "object" ? raw : null;
                    const seconds = window?.limit_window_seconds;
                    const label = seconds
                      ? seconds % 86400 === 0
                        ? `${seconds / 86400} 天`
                        : `${seconds / 3600} 小时`
                      : String(fallback);
                    return (
                      <div key={String(label)} className="space-y-3">
                        <CardTitle role="heading" aria-level={3}>
                          {String(label)}
                        </CardTitle>
                        {window ? (
                          <>
                            <div className="flex items-center justify-between gap-2">
                              <span>
                                {money(window.used_usd)} / {money(window.limit_usd)}
                              </span>
                              <strong>{window.used_percent}%</strong>
                            </div>
                            <Progress
                              value={window.used_percent}
                              aria-label={`${label}额度已用 ${window.used_percent}%`}
                            />
                            <CardDescription>重置：{date(window.reset_at)}</CardDescription>
                          </>
                        ) : (
                          <CardDescription className="text-sm text-muted-foreground">
                            {resource.data ? "未设置费用上限" : "—"}
                          </CardDescription>
                        )}
                      </div>
                    );
                  })}
              </div>
              {resource.data &&
                !resource.data.quota.rate_limit.primary_window &&
                !resource.data.quota.rate_limit.secondary_window && (
                  <CardDescription>当前套餐未设置费用上限。</CardDescription>
                )}
              <CardDescription className="text-sm text-muted-foreground">
                已结算费用 {money(resource.data?.quota.billing.used_usd)} · 待结算{" "}
                {resource.data?.quota.billing.pending_requests ?? "未提供"} 条 · 未计价{" "}
                {resource.data?.quota.billing.unpriced_requests ?? "未提供"} 条 · 历史未计费{" "}
                {resource.data?.quota.billing.legacy_requests ?? "未提供"} 条
              </CardDescription>
            </CardContent>
          </Card>
          <Card size="sm">
            <CardHeader>
              <CardTitle role="heading" aria-level={2}>
                {"每日实际 Token 用量"}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <>
                {(resource.data?.summary.daily_usage_buckets ?? []).length ? (
                  <div>
                    <ChartContainer
                      config={{ tokens: { label: "Token", color: "var(--primary)" } }}
                      className="h-48 w-full"
                      aria-label="每日实际 Token 用量"
                    >
                      <BarChart
                        accessibilityLayer
                        data={resource.data?.summary.daily_usage_buckets ?? []}
                        margin={{ left: 0, right: 12, top: 12, bottom: 0 }}
                      >
                        <CartesianGrid vertical={false} />
                        <XAxis
                          dataKey="start_date"
                          tickLine={false}
                          axisLine={false}
                          tickFormatter={(value: string) => value.slice(5)}
                        />
                        <YAxis
                          tickLine={false}
                          axisLine={false}
                          width={56}
                          tickFormatter={(value: number) => tokenCount(value)}
                        />
                        <ChartTooltip
                          content={
                            <ChartTooltipContent
                              formatter={(value) => (
                                <div className="flex flex-1 justify-between gap-3 leading-none">
                                  <span>Token</span>
                                  <span className="font-mono font-medium tabular-nums">
                                    {tokenCount(typeof value === "number" ? value : Number(value))}
                                  </span>
                                </div>
                              )}
                            />
                          }
                        />
                        <Bar
                          dataKey="tokens"
                          fill="var(--color-tokens)"
                          radius={[4, 4, 0, 0]}
                          maxBarSize={38}
                          isAnimationActive={false}
                        />
                      </BarChart>
                    </ChartContainer>
                    {(resource.data?.summary.daily_usage_buckets ?? []).filter(
                      (point) => point.tokens === null,
                    ).length > 0 && (
                      <CardDescription className="text-sm text-muted-foreground">
                        未上报用量：
                        {(resource.data?.summary.daily_usage_buckets ?? [])
                          .filter((point) => point.tokens === null)
                          .map((point) => point.start_date)
                          .join("、")}
                        。未知值不计为 0。
                      </CardDescription>
                    )}
                  </div>
                ) : (
                  <Empty>
                    <EmptyHeader>
                      <EmptyMedia variant="icon">
                        <Inbox />
                      </EmptyMedia>
                      <EmptyDescription>暂无实际用量</EmptyDescription>
                    </EmptyHeader>
                  </Empty>
                )}
              </>
            </CardContent>
          </Card>
        </div>
      }
      <UsagePageView consumerId={id} />
    </>
  );
}
