"use client";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useTablePagination } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight } from "lucide-react";

import { useDialogFocus, validateForm } from "@/lib/actions";
import { ScrollArea } from "@/components/ui/scroll-area";

import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
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
import { useActions, useErrorToast, copyElementText } from "@/lib/actions";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { Progress } from "@/components/ui/progress";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { date } from "@/lib/format";
import { Separator } from "@/components/ui/separator";
import Link from "next/link";
import { useEffect, useState } from "react";
import { useSupplierQuotas, useQuotaClock } from "@/hooks/use-supplier-quotas";
import {
  supplierStatusLabel,
  quotaWindowLabel,
  quotaResetLabel,
  percentLabel,
} from "@/lib/supplier-state";
import { duration, tokenCount } from "@/lib/usage-display";
import { Plus, Search, RotateCcw, RefreshCw, ArrowRight } from "lucide-react";
import {
  request,
  type Fingerprint,
  type List,
  type OAuth,
  type Proxy,
  type Supplier,
  type SupplierQuota,
  type Json,
} from "@/lib/api";
import { mergeOAuth } from "@/lib/domain";

import { useQueryId, useResource } from "@/lib/hooks";

export function SuppliersPage() {
  const fieldId = useId();
  const actions = useActions();
  const resource = useResource<List<Supplier>>("/suppliers");
  const [add, setAdd] = useState(false);
  const empty = { search: "", status: "" };
  const [filters, setFilters] = useState(empty);
  const [applied, setApplied] = useState(empty);
  const [view, setView] = useState<"table" | "cards">("table");
  const all = useSupplierQuotas(resource.data?.items);
  const now = useQuotaClock();
  const items = all.filter(
    (item) =>
      `${item.display_name ?? ""} ${item.email ?? ""} ${item.provider_id}`
        .toLowerCase()
        .includes(applied.search.trim().toLowerCase()) &&
      (!applied.status || item.status === applied.status),
  );
  const pagination = useTablePagination(items, applied, resource.data !== undefined);
  const supplierActions = (item: Supplier) => (
    <div className="flex flex-wrap items-center gap-2">
      <Button variant="outline" size="sm" asChild>
        <Link href={`/suppliers/detail/?id=${encodeURIComponent(item.id)}`}>
          详情 <ArrowRight />
        </Link>
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button type="button" variant="ghost" size="icon-sm" aria-label="更多操作">
            <MoreHorizontal />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {item.error_message && (
            <DropdownMenuItem
              disabled={actions.isBusy("recover-" + item.id)}
              onSelect={() =>
                void actions.run(
                  "recover-" + item.id,
                  async () => {
                    await request(`/suppliers/${item.id}/recover`, { method: "POST" });
                    resource.reload();
                  },
                  { success: "官方通信已恢复" },
                )
              }
            >
              恢复账户
            </DropdownMenuItem>
          )}

          <DropdownMenuItem
            variant={false ? "destructive" : "default"}
            disabled={false || actions.isBusy("components\\suppliers.tsx:action:1")}
            onSelect={() =>
              void actions.run(
                "components\\suppliers.tsx:action:1",
                async () => {
                  await request(`/suppliers/${item.id}/status`, {
                    method: "POST",
                    body: { enabled: item.status === "disabled" },
                  });
                  resource.reload();
                },
                {
                  confirm:
                    item.status !== "disabled"
                      ? "停用此供应账户？绑定账户将暂时无法通过它执行请求。"
                      : undefined,
                  danger: false,
                  success: undefined,
                },
              )
            }
          >
            {item.status !== "disabled" ? "停用账户" : "启用账户"}
          </DropdownMenuItem>
          <DropdownMenuItem
            variant={true ? "destructive" : "default"}
            disabled={false || actions.isBusy("components\\suppliers.tsx:action:2")}
            onSelect={() =>
              void actions.run(
                "components\\suppliers.tsx:action:2",
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
              resource.reload();
            }}
          >
            <Field className="w-44">
              <FieldLabel
                htmlFor={fieldId + "-field-3" + "-" + encodeURIComponent(String("搜索账户"))}
              >
                {"搜索账户"}
              </FieldLabel>
              <Input
                id={fieldId + "-field-3" + "-" + encodeURIComponent(String("搜索账户"))}
                aria-label={"搜索账户"}
                value={filters.search}
                onChange={(event) => setFilters({ ...filters, search: event.target.value })}
                placeholder="名称、邮箱或提供商"
              />
            </Field>
            <Field className="w-44">
              <FieldLabel
                htmlFor={fieldId + "-field-4" + "-" + encodeURIComponent(String("账户状态"))}
              >
                {"账户状态"}
              </FieldLabel>
              <Select
                value={filters.status}
                onValueChange={(next) =>
                  ((status) => setFilters({ ...filters, status }))(
                    next ===
                      fieldId + "-field-4" + "-" + encodeURIComponent(String("账户状态")) + "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-4" + "-" + encodeURIComponent(String("账户状态"))}
                  aria-label={"账户状态"}
                  data-required={false ? "true" : undefined}
                  data-empty={String(filters.status) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部状态" },
                        { value: "active", label: "启用" },
                        { value: "disabled", label: "停用" },
                        { value: "error", label: "错误" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部状态" },
                    { value: "active", label: "启用" },
                    { value: "disabled", label: "停用" },
                    { value: "error", label: "错误" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-4" +
                          "-" +
                          encodeURIComponent(String("账户状态")) +
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
                  resource.reload();
                }}
              >
                <RotateCcw />
                重置
              </Button>
            </div>
          </form>
          <div className="flex flex-wrap items-center gap-2 self-end xl:ml-auto">
            <Button type="button" variant="ghost" size="sm" onClick={resource.reload}>
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
              <Button type="button" onClick={() => setAdd(true)}>
                <Plus />
                添加供应账户
              </Button>
            }
          </div>
        </CardContent>
      </Card>
      {view === "table" ? (
        <Card>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  {["供应账户", "提供商 / 订阅", "状态", "额度", "最近使用", "操作"].map(
                    (label) => (
                      <TableHead key={label} scope="col">
                        {label}
                      </TableHead>
                    ),
                  )}
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.length ? (
                  <>
                    {pagination.rows.map((item) => (
                      <TableRow key={item.id}>
                        <TableCell>
                          <Link href={`/suppliers/detail/?id=${encodeURIComponent(item.id)}`}>
                            <strong>{item.display_name || item.email || item.id}</strong>
                          </Link>
                          <CardDescription>{item.email}</CardDescription>
                        </TableCell>
                        <TableCell>
                          {item.provider_id}
                          <CardDescription>{item.plan_type ?? "未提供"}</CardDescription>
                        </TableCell>
                        <TableCell>
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
                        <TableCell>
                          <div
                            className="flex w-64 flex-wrap gap-2"
                            aria-label="官方额度"
                            title={
                              item.quota ? "缓存于 " + date(item.quota.observed_at) : "暂无额度缓存"
                            }
                          >
                            {item.quota?.windows?.map((window) => (
                              <div
                                key={window.id}
                                className="min-w-0 flex-1 basis-28 space-y-1 text-xs"
                              >
                                <div
                                  className="truncate"
                                  title={quotaResetLabel(window?.reset_at, now)}
                                >
                                  {quotaWindowLabel(window)}：
                                  {quotaResetLabel(window.reset_at, now)}
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
                        <TableCell>{date(item.last_used_at)}</TableCell>
                        <TableCell>{supplierActions(item)}</TableCell>
                      </TableRow>
                    ))}
                  </>
                ) : (
                  <TableRow>
                    <TableCell
                      colSpan={
                        ["供应账户", "提供商 / 订阅", "状态", "额度", "最近使用", "操作"].length
                      }
                    >
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
                  <div className="min-w-0 break-words">
                    <Link href={`/suppliers/detail/?id=${encodeURIComponent(item.id)}`}>
                      <strong>{item.display_name || item.email || item.id}</strong>
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
                    { label: "上游订阅", value: item.plan_type ?? "未提供" },
                    { label: "最近使用", value: date(item.last_used_at) },
                  ].map(({ label, value }) => (
                    <Field
                      key={label}
                      orientation="horizontal"
                      className={label === "最近使用" ? "col-span-2 min-w-0" : "min-w-0"}
                    >
                      <FieldTitle className="shrink-0">{label}</FieldTitle>
                      <FieldDescription className="min-w-0 break-words">
                        {value ?? "—"}
                      </FieldDescription>
                    </Field>
                  ))}
                </FieldGroup>
                <div
                  className="flex w-full flex-wrap gap-2"
                  aria-label="官方额度"
                  title={item.quota ? "缓存于 " + date(item.quota.observed_at) : "暂无额度缓存"}
                >
                  {item.quota?.windows?.map((window) => (
                    <div key={window.id} className="min-w-0 flex-1 basis-28 space-y-1 text-xs">
                      <div className="truncate" title={quotaResetLabel(window?.reset_at, now)}>
                        {quotaWindowLabel(window)}：{quotaResetLabel(window.reset_at, now)}
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
  const [officialUsername, setOfficialUsername] = useState<string>();
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
  return (
    <>
      <div className="flex items-center justify-end gap-2">
        <CardDescription className="mr-auto truncate">
          {officialUsername ?? account?.username ?? "—"}
        </CardDescription>
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
              ["fingerprint", "指纹与网络"],
              ["quota", "官方额度"],
              ["local-usage", "本地用量"],
              ["usage", "官方用量"],
              ["details", "官方资料"],
              ["credits", "重置额度"],
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
                    { label: "上游订阅", value: account?.plan_type },
                    { label: "上游账户编号", value: account?.chatgpt_account_id },
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
                            body: { enabled: account?.status === "disabled" },
                          });
                          resource.reload();
                        },
                        {
                          confirm:
                            account?.status === "active"
                              ? "停用此供应账户？绑定账户将暂时无法通过它执行请求。"
                              : undefined,
                          danger: false,
                          success: undefined,
                        },
                      )
                    }
                  >
                    {account?.status !== "disabled" ? "停用" : "启用"}
                  </Button>
                </div>
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
          {["quota", "usage", "details", "credits"].includes(tab) && (
            <OfficialData
              key={tab}
              id={id}
              section={tab}
              onUsername={tab === "usage" ? setOfficialUsername : undefined}
            />
          )}
        </TabsContent>
      </Tabs>
      {relogin && (
        <OAuthWizard
          supplierId={id}
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
  const value = account?.usage ? ({ stats: account.usage } as unknown as Json) : null;
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          本地用量
        </CardTitle>
        <CardDescription>按本系统账本统计，计量维度与官方用量一致。</CardDescription>
      </CardHeader>
      <CardContent>
        <OfficialFields section="usage" value={value} />
      </CardContent>
    </Card>
  );
}
export function FingerprintFields({
  value,
  onChange,
  proxies,
}: {
  value: Fingerprint;
  onChange: (value: Fingerprint) => void;
  proxies: Proxy[];
}) {
  const fieldId = useId();
  const actions = useActions();
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      {(
        [
          ["os_type", "操作系统"],
          ["os_version", "系统版本"],
          ["arch", "架构"],
          ["terminal", "终端标识"],
        ] as const
      ).map(([key, label], fieldIndex7) => (
        <Field key={key}>
          <FieldLabel
            htmlFor={
              fieldId +
              "-field-8" +
              "-" +
              String(fieldIndex7) +
              "-" +
              encodeURIComponent(String(label))
            }
          >
            {label}
          </FieldLabel>
          <Input
            id={
              fieldId +
              "-field-8" +
              "-" +
              String(fieldIndex7) +
              "-" +
              encodeURIComponent(String(label))
            }
            aria-label={label}
            required
            maxLength={key === "terminal" ? 256 : 128}
            value={value[key]}
            onChange={(e) => onChange({ ...value, [key]: e.target.value })}
          />
        </Field>
      ))}
      <Field>
        <FieldLabel htmlFor={fieldId + "-field-9" + "-" + encodeURIComponent(String("出站代理"))}>
          {"出站代理"}
        </FieldLabel>
        <Select
          value={value.proxy_id ?? ""}
          onValueChange={(next) =>
            ((proxy_id) => onChange({ ...value, proxy_id: proxy_id || null }))(
              next ===
                fieldId + "-field-9" + "-" + encodeURIComponent(String("出站代理")) + "-empty"
                ? ""
                : next,
            )
          }
        >
          <SelectTrigger
            id={fieldId + "-field-9" + "-" + encodeURIComponent(String("出站代理"))}
            aria-label={"出站代理"}
            data-required={false ? "true" : undefined}
            data-empty={String(value.proxy_id ?? "") === "" ? "true" : undefined}
            className="w-full"
          >
            <SelectValue
              placeholder={
                [
                  { value: "", label: "不使用代理" },
                  ...proxies.map((proxy) => ({
                    value: proxy.id,
                    label: `${proxy.name} · ${proxy.display_url}`,
                  })),
                ].find((option) => option.value === "")?.label ?? "请选择"
              }
            />
          </SelectTrigger>
          <SelectContent position="popper">
            {[
              { value: "", label: "不使用代理" },
              ...proxies.map((proxy) => ({
                value: proxy.id,
                label: `${proxy.name} · ${proxy.display_url}`,
              })),
            ].map((option) => (
              <SelectItem
                key={option.value}
                value={
                  option.value ||
                  fieldId + "-field-9" + "-" + encodeURIComponent(String("出站代理")) + "-empty"
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
        <FieldLabel htmlFor={fieldId + "-field-10" + "-" + encodeURIComponent(String("时区"))}>
          {"时区"}
        </FieldLabel>
        <div className="flex items-center gap-2">
          <Input
            id={fieldId + "-field-10" + "-" + encodeURIComponent(String("时区"))}
            aria-label={"时区"}
            aria-describedby={
              fieldId + "-field-10" + "-" + encodeURIComponent(String("时区")) + "-hint"
            }
            value={value.timezone}
            onChange={(e) => onChange({ ...value, timezone: e.target.value })}
            placeholder="Asia/Taipei"
          />
          {value.proxy_id && proxies.some((proxy) => proxy.id === value.proxy_id) && (
            <Button
              type="button"
              variant={false ? "destructive" : "outline"}
              className="shrink-0"
              disabled={false || actions.isBusy("components\\suppliers.tsx:action:11")}
              onClick={() =>
                void actions.run(
                  "components\\suppliers.tsx:action:11",
                  async () => {
                    const result = await request<{ timezone: string }>(
                      `/proxies/${value.proxy_id}/check/timezone`,
                      { method: "POST", body: {} },
                    );
                    onChange({ ...value, timezone: result.timezone });
                  },
                  { confirm: undefined, danger: false, success: undefined },
                )
              }
            >
              应用代理时区
            </Button>
          )}
        </div>
        {Boolean("留空保留请求中的时区。") && (
          <FieldDescription
            id={fieldId + "-field-10" + "-" + encodeURIComponent(String("时区")) + "-hint"}
          >
            {"留空保留请求中的时区。"}
          </FieldDescription>
        )}
      </Field>
    </div>
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
  const proxies = useResource<List<Proxy>>("/proxies");
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
                    <CardDescription>每个供应账户独立保存系统、终端和网络设置。</CardDescription>
                  </div>
                  <FingerprintFields
                    value={value}
                    onChange={setChanges}
                    proxies={proxies.data?.items ?? []}
                  />
                </section>

                <CardDescription className="text-sm text-muted-foreground">
                  指纹保存到该供应账户，后续请求使用此身份。
                </CardDescription>
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
  onClose,
  onComplete,
}: {
  supplierId?: string;
  onClose: () => void;
  onComplete: () => void;
}) {
  const dialogFocus = useDialogFocus();
  const fieldId = useId();
  const actions = useActions();
  const setup = useResource<{ fingerprint: Fingerprint }>(
    supplierId ? null : "/suppliers/oauth/setup",
  );
  const proxies = useResource<List<Proxy>>(supplierId ? null : "/proxies");
  useErrorToast(setup.error);
  useErrorToast(proxies.error);
  const [step, setStep] = useState(supplierId ? 1 : 0);
  const [fingerprint, setFingerprint] = useState<Fingerprint>();
  const [method, setMethod] = useState<"callback" | "device" | "refresh_token">("callback");
  const [refreshToken, setRefreshToken] = useState("");
  const [flow, setFlow] = useState<OAuth>();
  const [callback, setCallback] = useState("");
  const [pollAfter, setPollAfter] = useState(0);
  const pending = flow?.status === "pending" ? flow : undefined;
  const busy = actions.running.size > 0;
  const ready = Boolean(supplierId) || (setup.ready && proxies.ready);
  const methods = [
    { value: "callback", label: "回调链接", description: "打开授权页面后提交完整回调链接" },
    { value: "device", label: "设备码", description: "在设备授权页面输入一次性代码" },
    ...(!supplierId
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
    if (value.status === "complete") onComplete();
    else setPollAfter(Date.now() + (value.interval ?? 5) * 1000);
  };
  const cancelPending = async () => {
    if (pending) {
      await request("/suppliers/oauth/cancel", { method: "POST", body: { state: pending.state } });
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
        onClose();
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
    accept(
      await request<OAuth>(
        supplierId ? `/suppliers/${supplierId}/relogin` : "/suppliers/oauth/start",
        {
          method: "POST",
          body: supplierId
            ? { method }
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
        await request<OAuth>("/suppliers/oauth/callback", {
          method: "POST",
          body: { state: pending.state, callback_url: callback },
        }),
      );
    } else {
      if (Date.now() < pollAfter) throw new Error("请完成官方授权后稍等片刻再检查。");
      accept(
        await request<OAuth>("/suppliers/oauth/poll", {
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
                  <FingerprintFields
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
                      <Input
                        id={`${fieldId}-refresh-token`}
                        type="password"
                        autoComplete="off"
                        required
                        value={refreshToken}
                        onChange={(event) => setRefreshToken(event.target.value)}
                      />
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
                            授权后的完整回调地址
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
              </TabsContent>
            </ScrollArea>
            <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
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

function OfficialData({
  id,
  section,
  onUsername,
}: {
  id: string;
  section: string;
  onUsername?: (username: string | undefined) => void;
}) {
  const [refresh, setRefresh] = useState(0);
  const resource = useResource<{
    value: Json;
    quota?: SupplierQuota;
    observed_at?: string;
    refresh_error?: string | null;
    routing?: {
      status: "not_observed" | "ready" | "stale" | "invalid";
      backend_origin: string | null;
      constraint: "NO_CONSTRAINT" | "us" | "us_cr" | null;
      message: string | null;
    };
  }>(`/suppliers/${id}/official?section=${section}${refresh ? `&refresh=true&r=${refresh}` : ""}`);
  const root = resource.data?.value;
  const value = root && typeof root === "object" && !Array.isArray(root) ? root : {};
  const username =
    value.profile && typeof value.profile === "object" && !Array.isArray(value.profile)
      ? typeof value.profile.username === "string"
        ? value.profile.username
        : undefined
      : undefined;
  useEffect(() => {
    if (section !== "usage") return;
    onUsername?.(username);
  }, [onUsername, section, username]);
  useErrorToast(resource.error);
  useErrorToast(resource.data?.refresh_error ?? undefined);
  return (
    <Card>
      <CardHeader>
        <CardTitle
          role="heading"
          aria-level={2}
        >{`官方${({ quota: "额度", usage: "用量", details: "资料", credits: "重置额度" } as Record<string, string>)[section]}`}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" onClick={() => setRefresh(Date.now())}>
            从官方刷新
          </Button>
          {resource.data?.observed_at && (
            <CardDescription>采集时间：{date(resource.data?.observed_at)}</CardDescription>
          )}
        </div>

        {section === "details" && (
          <FieldGroup className="grid gap-3 sm:grid-cols-3">
            <Field>
              <FieldTitle>官方执行地址</FieldTitle>
              <FieldDescription>{resource.data?.routing?.backend_origin ?? "—"}</FieldDescription>
            </Field>
            <Field>
              <FieldTitle>区域约束</FieldTitle>
              <FieldDescription>
                {resource.data?.routing?.constraint
                  ? { NO_CONSTRAINT: "无区域约束", us: "美国", us_cr: "美国（us_cr）" }[
                      resource.data.routing.constraint
                    ]
                  : "—"}
              </FieldDescription>
            </Field>
            <Field>
              <FieldTitle>路由状态</FieldTitle>
              <FieldDescription>
                {resource.data?.routing
                  ? {
                      not_observed: "尚未采集",
                      ready: "可用",
                      stale: "凭据已变更，待重新采集",
                      invalid: "官方路由资料无效",
                    }[resource.data.routing.status]
                  : "—"}
                {resource.data?.routing?.message && `：${resource.data.routing.message}`}
              </FieldDescription>
            </Field>
          </FieldGroup>
        )}

        {
          <>
            {section === "quota" && (
              <div className="grid gap-4 sm:grid-cols-2">
                {resource.data?.quota?.windows?.map((window) => (
                  <div key={window.id} className="space-y-3">
                    <CardTitle role="heading" aria-level={3}>
                      {quotaWindowLabel(window)}
                    </CardTitle>
                    <div>
                      已用{" "}
                      {window.used_percent != null ? percentLabel(window.used_percent) : "未知"}
                    </div>
                    {window.used_percent != null && (
                      <Progress
                        value={Math.min(100, window.used_percent)}
                        aria-label={`官方额度已用 ${window.used_percent}%`}
                      />
                    )}
                    <CardDescription>
                      重置：
                      {date(window.reset_at)}
                    </CardDescription>
                  </div>
                ))}
                {!resource.data?.quota?.windows?.length && (
                  <CardDescription>
                    {resource.data?.quota?.windows ? "官方未提供额度窗口" : "额度数据未加载"}
                  </CardDescription>
                )}
              </div>
            )}
            {section === "credits" ? (
              <OfficialCredits value={value} id={id} onRefresh={() => setRefresh(Date.now())} />
            ) : (
              section !== "quota" && (
                <OfficialFields section={section} value={resource.data?.value ?? null} />
              )
            )}
          </>
        }
      </CardContent>
    </Card>
  );
}
function OfficialFields({ value, section }: { value: Json; section: string }) {
  const root = value && typeof value === "object" && !Array.isArray(value) ? value : {};
  const profile =
    root.profile && typeof root.profile === "object" && !Array.isArray(root.profile)
      ? root.profile
      : root;
  const statsValue =
    root.stats && typeof root.stats === "object" && !Array.isArray(root.stats)
      ? root.stats
      : profile.stats && typeof profile.stats === "object" && !Array.isArray(profile.stats)
        ? profile.stats
        : {};
  const stats =
    statsValue.stats && typeof statsValue.stats === "object" && !Array.isArray(statsValue.stats)
      ? statsValue.stats
      : statsValue;
  const rows = Array.isArray(root.accounts)
    ? root.accounts
    : root.accounts && typeof root.accounts === "object"
      ? Object.values(root.accounts)
      : [];
  const pagination = useTablePagination(rows, section);
  const days = Array.isArray(stats.daily_usage_buckets) ? stats.daily_usage_buckets : [];
  const fields =
    section === "usage"
      ? [
          {
            label: "累计 Token",
            value: tokenCount(
              typeof stats.lifetime_tokens === "number" ? stats.lifetime_tokens : null,
            ),
          },
          {
            label: "单日最高 Token",
            value: tokenCount(
              typeof stats.peak_daily_tokens === "number" ? stats.peak_daily_tokens : null,
            ),
          },
          {
            label: "当前连续使用天数",
            value:
              typeof stats.current_streak_days === "number"
                ? `${stats.current_streak_days}天`
                : "—",
          },
          {
            label: "最长连续使用天数",
            value:
              typeof stats.longest_streak_days === "number"
                ? `${stats.longest_streak_days}天`
                : "—",
          },
          {
            label: "最长任务时长",
            value:
              typeof stats.longest_running_turn_sec === "number"
                ? duration(stats.longest_running_turn_sec * 1000)
                : "—",
          },
        ]
      : [{ label: "默认账户", value: root.default_account_id }];
  return (
    <div className="space-y-4">
      <FieldGroup className="grid gap-3 sm:grid-cols-2">
        {fields.map((field) => (
          <Field key={field.label}>
            <FieldTitle>{field.label}</FieldTitle>
            <FieldDescription>
              {field.value == null
                ? "—"
                : typeof field.value === "string" || typeof field.value === "number"
                  ? field.value
                  : String(field.value)}
            </FieldDescription>
          </Field>
        ))}
      </FieldGroup>
      {section === "usage" ? (
        <ChartContainer
          config={{ tokens: { label: "Token", color: "var(--primary)" } }}
          className="h-60 w-full"
        >
          <BarChart accessibilityLayer data={days}>
            <CartesianGrid vertical={false} />
            <XAxis dataKey="start_date" tickLine={false} axisLine={false} />
            <YAxis
              tickLine={false}
              axisLine={false}
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
              isAnimationActive={false}
            />
          </BarChart>
        </ChartContainer>
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                {["账户", "类型", "订阅"].map((label) => (
                  <TableHead key={label}>{label}</TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.length ? (
                pagination.rows.map((raw, index) => {
                  const item = raw && typeof raw === "object" && !Array.isArray(raw) ? raw : {};
                  const account =
                    item.account && typeof item.account === "object" && !Array.isArray(item.account)
                      ? item.account
                      : item;
                  return (
                    <TableRow key={index}>
                      <TableCell>
                        {String(account.name ?? account.account_id ?? account.id ?? "—")}
                      </TableCell>
                      <TableCell>{String(account.structure ?? "—")}</TableCell>
                      <TableCell>{String(account.plan_type ?? "—")}</TableCell>
                    </TableRow>
                  );
                })
              ) : (
                <TableRow>
                  <TableCell colSpan={3}>
                    <Empty>
                      <EmptyDescription>暂无账户资料</EmptyDescription>
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
        </>
      )}
    </div>
  );
}
function OfficialCredits({
  value,
  id,
  onRefresh,
}: {
  value: { [key: string]: Json };
  id: string;
  onRefresh: () => void;
}) {
  const actions = useActions();
  const credits = Array.isArray(value.credits)
    ? value.credits.filter(
        (credit): credit is { [key: string]: Json } =>
          Boolean(credit) && typeof credit === "object" && !Array.isArray(credit),
      )
    : [];
  const pagination = useTablePagination(credits, id);
  const consume = async (creditId?: string) => {
    await request(`/suppliers/${id}/credits/consume`, {
      method: "POST",
      body: creditId ? { credit_id: creditId } : {},
    });
    onRefresh();
  };
  return (
    <div className="space-y-4">
      <CardDescription>
        可用次数：{typeof value.available_count === "number" ? value.available_count : "官方未提供"}
      </CardDescription>
      <Table>
        <TableHeader>
          <TableRow>
            {["名称", "类型", "状态", "到期时间", "操作"].map((label) => (
              <TableHead key={label} scope="col">
                {label}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {credits.length ? (
            <>
              {pagination.rows.map((credit, index) => (
                <TableRow key={String(credit.id ?? index)}>
                  <TableCell>{String(credit.title ?? credit.id ?? "—")}</TableCell>
                  <TableCell>{String(credit.reset_type ?? "—")}</TableCell>
                  <TableCell>{String(credit.status ?? "—")}</TableCell>
                  <TableCell>
                    {date(typeof credit.expires_at === "string" ? credit.expires_at : null)}
                  </TableCell>
                  <TableCell>
                    {credit.status === "available" && typeof credit.id === "string" ? (
                      <Button
                        type="button"
                        variant={false ? "destructive" : "outline"}
                        disabled={false || actions.isBusy("components\\suppliers.tsx:action:20")}
                        onClick={() =>
                          void actions.run(
                            "components\\suppliers.tsx:action:20",
                            () => consume(String(credit.id)),
                            {
                              confirm: "使用此供应账户的一次官方重置额度？",
                              danger: false,
                              success: undefined,
                            },
                          )
                        }
                      >
                        使用重置额度
                      </Button>
                    ) : (
                      "—"
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </>
          ) : (
            <TableRow>
              <TableCell colSpan={["名称", "类型", "状态", "到期时间", "操作"].length}>
                <Empty>
                  <EmptyDescription>{"暂无记录"}</EmptyDescription>
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
      {typeof value.available_count === "number" && value.available_count > 0 && (
        <Button
          type="button"
          variant={false ? "destructive" : "outline"}
          disabled={false || actions.isBusy("components\\suppliers.tsx:action:21")}
          onClick={() =>
            void actions.run("components\\suppliers.tsx:action:21", () => consume(), {
              confirm: "使用此供应账户的一次官方重置额度？",
              danger: false,
              success: undefined,
            })
          }
        >
          使用一次可用重置额度
        </Button>
      )}
    </div>
  );
}
