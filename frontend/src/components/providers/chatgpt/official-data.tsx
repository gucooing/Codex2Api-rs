"use client";

import { subscriptionLabel } from "@/lib/subscriptions";

import { useColumnVisibility } from "@/lib/columns";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useListResource } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight, Columns3 } from "lucide-react";

import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart";
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

import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";

import { Input } from "@/components/ui/input";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";

import {
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableBody,
  TableCell,
} from "@/components/ui/table";
import { Empty, EmptyDescription } from "@/components/ui/empty";

import { useActions, useErrorToast } from "@/lib/actions";

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DialogDescription,
} from "@/components/ui/dialog";

import { Progress } from "@/components/ui/progress";

import { date } from "@/lib/format";

import { useEffect, useState } from "react";

import { quotaWindowLabel, percentLabel } from "@/lib/supplier-state";
import { duration, tokenCount } from "@/lib/usage-display";

import { request, type SupplierQuota, type Json } from "@/lib/api";

import { ResourceRefreshContext, useResource } from "@/lib/hooks";

export function ChatgptOfficialData({
  id,
  section,
  onUpdated,
}: {
  id: string;
  section: string;
  onUpdated?: () => void;
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
  useErrorToast(resource.error);
  useErrorToast(resource.data?.refresh_error ?? undefined);
  useEffect(() => {
    if (section === "quota" && resource.data) onUpdated?.();
  }, [section, resource.data, onUpdated]);
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
            <ResourceRefreshContext value={Date.parse(resource.data?.observed_at ?? "") || 0}>
              {section === "credits" ? (
                <OfficialCredits value={value} id={id} onRefresh={() => setRefresh(Date.now())} />
              ) : (
                section !== "quota" && (
                  <OfficialFields id={id} section={section} value={resource.data?.value ?? null} />
                )
              )}
            </ResourceRefreshContext>
          </>
        }
      </CardContent>
    </Card>
  );
}
function OfficialFields({ value, section, id }: { value: Json; section: string; id: string }) {
  const tableColumns1 = useColumnVisibility(
    "components/suppliers.tsx:1",
    ["账户", "类型", "订阅"],
    ["账户", "类型", "订阅"],
  );

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
  const resource = useListResource<Json>(
    section === "details" ? `/suppliers/chatgpt/${id}/official/rows` : null,
    { kind: "details" },
  );
  const pagination = resource.pagination;
  const rows = pagination.rows;
  useErrorToast(resource.error);
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
              cursor={false}
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
              maxBarSize={48}
              fill="var(--color-tokens)"
              radius={[4, 4, 0, 0]}
              isAnimationActive={false}
            />
          </BarChart>
        </ChartContainer>
      ) : (
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
                    {tableColumns1.mobile ? "手机显示列" : "桌面显示列"}
                  </DropdownMenuLabel>
                  {tableColumns1.labels.map((label) => (
                    <DropdownMenuCheckboxItem
                      key={label}
                      checked={tableColumns1.isVisible(label)}
                      disabled={tableColumns1.count === 1 && tableColumns1.isVisible(label)}
                      onSelect={(event) => event.preventDefault()}
                      onCheckedChange={(checked) =>
                        tableColumns1.setVisible(label, checked === true)
                      }
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
                  {["账户", "类型", "订阅"].map((label) => (
                    <TableHead
                      hidden={!tableColumns1.isVisible(label)}
                      className={
                        ["账户", "类型", "订阅"].includes(label)
                          ? label === "账户"
                            ? ""
                            : "max-md:w-16"
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
                {rows.length ? (
                  pagination.rows.map((raw, index) => {
                    const item = raw && typeof raw === "object" && !Array.isArray(raw) ? raw : {};
                    const account =
                      item.account &&
                      typeof item.account === "object" &&
                      !Array.isArray(item.account)
                        ? item.account
                        : item;
                    return (
                      <TableRow role="row" key={index}>
                        <TableCell
                          hidden={!tableColumns1.isVisible("账户")}
                          className=" max-md:overflow-hidden"
                          data-label="账户"
                          role="cell"
                        >
                          <div className="max-md:hidden">
                            {String(account.name ?? account.account_id ?? account.id ?? "—")}
                          </div>
                          <Dialog>
                            <DialogTrigger asChild>
                              <Button
                                type="button"
                                variant="ghost"
                                className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                                aria-label={
                                  "查看详情：" +
                                  String(
                                    String(account.name ?? account.account_id ?? account.id ?? "—"),
                                  )
                                }
                              >
                                <span className="min-w-0 flex-1">
                                  <span className="block truncate font-medium">
                                    {String(
                                      account.name ?? account.account_id ?? account.id ?? "—",
                                    )}
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
                                  <FieldTitle>账户</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    {String(
                                      account.name ?? account.account_id ?? account.id ?? "—",
                                    )}
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>类型</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    {String(account.structure ?? "—")}
                                  </div>
                                </Field>
                                <Field>
                                  <FieldTitle>订阅</FieldTitle>
                                  <div className="min-w-0 break-words [&_*]:max-w-full">
                                    {subscriptionLabel(account.plan_type)}
                                  </div>
                                </Field>
                              </FieldGroup>
                            </DialogContent>
                          </Dialog>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns1.isVisible("类型")}
                          className=" max-md:overflow-hidden"
                          data-label="类型"
                          data-compact="true"
                          role="cell"
                        >
                          {String(account.structure ?? "—")}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns1.isVisible("订阅")}
                          className=" max-md:overflow-hidden"
                          data-label="订阅"
                          role="cell"
                        >
                          {subscriptionLabel(account.plan_type)}
                        </TableCell>
                      </TableRow>
                    );
                  })
                ) : (
                  <TableRow role="row">
                    <TableCell role="cell" colSpan={tableColumns1.count}>
                      <Empty>
                        <EmptyDescription>暂无账户资料</EmptyDescription>
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
  const tableColumns2 = useColumnVisibility(
    "components/suppliers.tsx:2",
    ["名称", "类型", "状态", "到期时间", "操作"],
    ["名称", "状态", "操作"],
  );

  const actions = useActions();
  const resource = useListResource<{ [key: string]: Json }>(
    `/suppliers/chatgpt/${id}/official/rows`,
    { kind: "credits" },
  );
  const pagination = resource.pagination;
  const credits = pagination.rows;
  useErrorToast(resource.error);
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
                  onCheckedChange={(checked) => tableColumns2.setVisible(label, checked === true)}
                >
                  {label}
                </DropdownMenuCheckboxItem>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={tableColumns2.showAll}>显示全部列</DropdownMenuItem>
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
              {["名称", "类型", "状态", "到期时间", "操作"].map((label) => (
                <TableHead
                  hidden={!tableColumns2.isVisible(label)}
                  className={
                    ["名称", "状态", "操作"].includes(label)
                      ? label === "操作"
                        ? "max-md:w-28"
                        : label === "名称"
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
            {credits.length ? (
              <>
                {pagination.rows.map((credit, index) => (
                  <TableRow role="row" key={String(credit.id ?? index)}>
                    <TableCell
                      hidden={!tableColumns2.isVisible("名称")}
                      className=" max-md:overflow-hidden"
                      data-label="名称"
                      role="cell"
                    >
                      <div className="max-md:hidden">
                        {String(credit.title ?? credit.id ?? "—")}
                      </div>
                      <Dialog>
                        <DialogTrigger asChild>
                          <Button
                            type="button"
                            variant="ghost"
                            className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                            aria-label={
                              "查看详情：" + String(String(credit.title ?? credit.id ?? "—"))
                            }
                          >
                            <span className="min-w-0 flex-1">
                              <span className="block truncate font-medium">
                                {String(credit.title ?? credit.id ?? "—")}
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
                              <FieldTitle>名称</FieldTitle>
                              <div className="min-w-0 break-words [&_*]:max-w-full">
                                {String(credit.title ?? credit.id ?? "—")}
                              </div>
                            </Field>
                            <Field>
                              <FieldTitle>类型</FieldTitle>
                              <div className="min-w-0 break-words [&_*]:max-w-full">
                                {String(credit.reset_type ?? "—")}
                              </div>
                            </Field>
                            <Field>
                              <FieldTitle>状态</FieldTitle>
                              <div className="min-w-0 break-words [&_*]:max-w-full">
                                {String(credit.status ?? "—")}
                              </div>
                            </Field>
                            <Field>
                              <FieldTitle>到期时间</FieldTitle>
                              <div className="min-w-0 break-words [&_*]:max-w-full">
                                {date(
                                  typeof credit.expires_at === "string" ? credit.expires_at : null,
                                )}
                              </div>
                            </Field>
                          </FieldGroup>
                        </DialogContent>
                      </Dialog>
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns2.isVisible("类型")}
                      className=" "
                      data-label="类型"
                      data-compact="true"
                      role="cell"
                    >
                      {String(credit.reset_type ?? "—")}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns2.isVisible("状态")}
                      className=" max-md:overflow-hidden"
                      data-label="状态"
                      data-compact="true"
                      role="cell"
                    >
                      {String(credit.status ?? "—")}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns2.isVisible("到期时间")}
                      className=" "
                      data-label="到期时间"
                      role="cell"
                    >
                      {date(typeof credit.expires_at === "string" ? credit.expires_at : null)}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns2.isVisible("操作")}
                      className=" max-md:[&_button]:h-7 max-md:[&_button]:px-1.5 max-md:[&_button]:text-xs max-md:[&_button]:gap-1 max-md:[&_a]:h-7 max-md:[&_a]:px-1.5 max-md:[&_a]:text-xs max-md:[&_a]:gap-1 max-md:[&>div]:gap-1"
                      data-label="操作"
                      role="cell"
                    >
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
