"use client";

import { useMemo, useState } from "react";
import { Bar, CartesianGrid, ComposedChart, Line, XAxis, YAxis } from "recharts";
import { RefreshCw, RotateCcw, Search } from "lucide-react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import { useResource } from "@/lib/hooks";
import { usePreference, useSavedFilters } from "@/lib/preferences";
import { query, type Consumer, type Supplier, type List } from "@/lib/api";
import { toastError, useErrorToast, validateForm } from "@/lib/actions";
import { tokenCount, usageStatuses } from "@/lib/usage-display";
import { useUserLookup, userOptionLabel } from "@/lib/user-lookup";
import {
  percent,
  overviewChart,
  statisticsRange,
  usd,
  usageGroups,
  type UsageGroup,
  type UsageStatistics,
} from "@/lib/usage-statistics";

const defaults = {
  user_id: "",
  user_label: "",
  virtual_account: "",
  consumer_label: "",
  supplier_id: "",
  supplier_label: "",
  model: "",
  status: "",
  group_by: "day" as UsageGroup,
  preset: "7",
  from: "",
  until: "",
};
type SubjectOption = { id: string; kind: "user" | "virtual_account"; label: string };
type SupplierOption = Pick<Supplier, "id" | "display_name" | "email">;
const subjectOptionLabel = (item: SubjectOption) =>
  `${item.kind === "user" ? "用户" : "账户"} · ${item.label}`;
const supplierOptionLabel = (item: SupplierOption) => item.display_name || item.email || item.id;
const chartConfig = {
  total_tokens: { label: "Token 用量", color: "var(--chart-1)" },
  request_count: { label: "请求数", color: "var(--chart-2)" },
  cache_rate: { label: "缓存率", color: "var(--chart-3)" },
  cost_usd: { label: "费用（USD）", color: "var(--chart-4)" },
};
const seriesOptions = {
  models: "模型用量（柱体）",
  total_tokens: "Token 用量",
  request_count: "请求数",
  cache_rate: "缓存率",
  cost_usd: "费用",
};
const defaultVisibility = {
  models: true,
  total_tokens: true,
  request_count: true,
  cache_rate: true,
  cost_usd: true,
};

export function OverviewStatistics({ onRefresh }: { onRefresh: () => void }) {
  const {
    filters,
    setFilters,
    applied: saved,
    setApplied,
    ready: preferencesReady,
  } = useSavedFilters(
    "overview.filters",
    defaults,
    (value) =>
      ["today", "1", "7", "30", "custom"].includes(value.preset) &&
      Object.hasOwn(usageGroups, value.group_by) &&
      !(value.user_id && value.virtual_account),
  );
  const applied = useMemo(
    () => ({ ...saved, ...(saved.preset === "custom" ? {} : statisticsRange(saved.preset)) }),
    [saved],
  );
  const [visible, setVisible] = usePreference("overview.series", defaultVisibility);
  const userLookup = useUserLookup();
  const [supplierOpen, setSupplierOpen] = useState(false);
  const [supplierSearch, setSupplierSearch] = useState("");
  const selectedSubject: SubjectOption | null = filters.user_id
    ? { id: filters.user_id, kind: "user", label: filters.user_label || filters.user_id }
    : filters.virtual_account
      ? {
          id: filters.virtual_account,
          kind: "virtual_account",
          label: filters.consumer_label || filters.virtual_account,
        }
      : null;
  const selectedSupplier = filters.supplier_id
    ? { id: filters.supplier_id, display_name: filters.supplier_label, email: "" }
    : null;
  const consumers = useResource<List<Consumer>>(
    userLookup.open
      ? `/consumers/options${query({ search: userLookup.search.trim(), limit: 5 })}`
      : null,
    userLookup.search.trim() ? 250 : 0,
  );
  const subjects: SubjectOption[] = [
    ...(userLookup.data?.items ?? []).map((item) => ({
      id: item.id,
      kind: "user" as const,
      label: userOptionLabel(item),
    })),
    ...(consumers.data?.items ?? []).map((item) => ({
      id: item.id,
      kind: "virtual_account" as const,
      label: item.username,
    })),
  ];
  const suppliers = useResource<List<Supplier>>(
    supplierOpen ? `/suppliers/options${query({ search: supplierSearch.trim(), limit: 5 })}` : null,
    supplierSearch.trim() ? 250 : 0,
  );
  const path = `/overview/usage${query({ user_id: applied.user_id, virtual_account: applied.virtual_account, supplier_id: applied.supplier_id, model: applied.model, status: applied.status, group_by: applied.group_by, from: applied.from, until: applied.until, tz_offset: new Date().getTimezoneOffset() })}`;
  const resource = useResource<UsageStatistics>(preferencesReady ? path : null);
  useErrorToast(resource.error);
  useErrorToast(consumers.error);
  useErrorToast(suppliers.error);
  const update = (key: keyof typeof filters, value: string) =>
    setFilters((current) => ({ ...current, [key]: value }));
  const apply = (values: typeof filters) => {
    const range =
      values.preset === "custom"
        ? { from: values.from, until: values.until }
        : statisticsRange(values.preset);
    const span = new Date(range.until).getTime() - new Date(range.from).getTime();
    const maxDays = values.group_by === "hour" ? 31 : 366;
    if (!Number.isFinite(span) || span <= 0 || span > maxDays * 86_400_000) {
      toastError(`请选择有效时间范围，结束时间需晚于开始时间，当前分组最多查询 ${maxDays} 天`);
      return;
    }
    setApplied(
      values.preset === "custom" ? { ...values, ...range } : { ...values, from: "", until: "" },
    );
    resource.reload();
  };
  const summary = resource.data?.summary;
  const timeSeries = applied.group_by === "day" || applied.group_by === "hour";
  const chart = overviewChart(resource.data);
  const config: ChartConfig = {
    ...chartConfig,
    ...Object.fromEntries(
      chart.models.map((model) => [model.key, { label: model.label, color: model.color }]),
    ),
  };

  return (
    <>
      <Card>
        <CardContent>
          <form
            aria-label="概览统计筛选"
            className="flex flex-wrap items-end gap-3"
            noValidate
            onSubmit={(event) => {
              event.preventDefault();
              if (validateForm(event.currentTarget)) apply(filters);
            }}
          >
            <Field className="w-40">
              <FieldLabel htmlFor="overview-subject">用户/账户</FieldLabel>
              <Combobox<SubjectOption>
                items={subjects}
                value={selectedSubject}
                onValueChange={(item) => {
                  setFilters((current) => ({
                    ...current,
                    user_id: item?.kind === "user" ? item.id : "",
                    user_label: item?.kind === "user" ? item.label : "",
                    virtual_account: item?.kind === "virtual_account" ? item.id : "",
                    consumer_label: item?.kind === "virtual_account" ? item.label : "",
                  }));
                  userLookup.setSearch("");
                }}
                itemToStringLabel={subjectOptionLabel}
                itemToStringValue={(item) => `${item.kind}:${item.id}`}
                isItemEqualToValue={(item, value) =>
                  item.kind === value.kind && item.id === value.id
                }
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
                      setFilters((current) => ({
                        ...current,
                        user_id: "",
                        user_label: "",
                        virtual_account: "",
                        consumer_label: "",
                      }));
                    }
                  }
                }}
              >
                <ComboboxInput
                  id="overview-subject"
                  placeholder="全部用户/账户"
                  showClear
                  className="w-full"
                />
                <ComboboxContent>
                  <ComboboxEmpty>
                    {userLookup.loading || consumers.loading
                      ? "正在加载…"
                      : userLookup.error || consumers.error
                        ? "加载失败，请重新搜索"
                        : "没有匹配用户/账户"}
                  </ComboboxEmpty>
                  <ComboboxList aria-busy={userLookup.loading || consumers.loading}>
                    {(item: SubjectOption) => (
                      <ComboboxItem key={`${item.kind}:${item.id}`} value={item}>
                        {subjectOptionLabel(item)}
                      </ComboboxItem>
                    )}
                  </ComboboxList>
                </ComboboxContent>
              </Combobox>
            </Field>
            <Field className="w-40">
              <FieldLabel htmlFor="overview-supplier">供应账户</FieldLabel>
              <Combobox<SupplierOption>
                items={suppliers.data?.items ?? []}
                value={selectedSupplier}
                onValueChange={(item) => {
                  setFilters((current) => ({
                    ...current,
                    supplier_id: item?.id ?? "",
                    supplier_label: item ? supplierOptionLabel(item) : "",
                  }));
                  setSupplierSearch("");
                }}
                itemToStringLabel={supplierOptionLabel}
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
                  id="overview-supplier"
                  placeholder="全部供应账户"
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
                          <span className="truncate">{supplierOptionLabel(item)}</span>
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
            <Field className="w-40">
              <FieldLabel htmlFor="overview-model">模型</FieldLabel>
              <Input
                id="overview-model"
                placeholder="请求／响应模型关键词"
                value={filters.model}
                onChange={(event) => update("model", event.target.value)}
              />
            </Field>
            <Field className="w-32">
              <FieldLabel htmlFor="overview-range">时间范围</FieldLabel>
              <Select
                value={filters.preset}
                onValueChange={(value) =>
                  setFilters((current) => ({
                    ...current,
                    preset: value,
                    ...(value === "custom"
                      ? statisticsRange(current.preset === "custom" ? "7" : current.preset)
                      : {}),
                  }))
                }
              >
                <SelectTrigger id="overview-range" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    ["today", "今天"],
                    ["1", "最近 24 小时"],
                    ["7", "最近 7 天"],
                    ["30", "最近 30 天"],
                    ["custom", "自定义"],
                  ].map(([value, label]) => (
                    <SelectItem key={value} value={value}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            {filters.preset === "custom" && (
              <>
                <Field className="w-52">
                  <FieldLabel htmlFor="overview-from">开始时间</FieldLabel>
                  <Input
                    id="overview-from"
                    type="datetime-local"
                    required
                    value={filters.from}
                    onChange={(event) => update("from", event.target.value)}
                  />
                </Field>
                <Field className="w-52">
                  <FieldLabel htmlFor="overview-until">结束时间（不含）</FieldLabel>
                  <Input
                    id="overview-until"
                    type="datetime-local"
                    required
                    value={filters.until}
                    onChange={(event) => update("until", event.target.value)}
                  />
                </Field>
              </>
            )}
            <Field className="w-28">
              <FieldLabel htmlFor="overview-status">请求状态</FieldLabel>
              <Select
                value={filters.status || "all"}
                onValueChange={(value) => update("status", value === "all" ? "" : value)}
              >
                <SelectTrigger id="overview-status" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="all">全部状态</SelectItem>
                  {usageStatuses.map((status) => (
                    <SelectItem key={status.value} value={status.value}>
                      {status.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field className="w-36">
              <FieldLabel htmlFor="overview-group">统计维度</FieldLabel>
              <Select value={filters.group_by} onValueChange={(value) => update("group_by", value)}>
                <SelectTrigger id="overview-group" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  {Object.entries(usageGroups).map(([value, label]) => (
                    <SelectItem key={value} value={value}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Button type="submit" size="sm" disabled={resource.refreshing}>
              <Search />
              查询
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                setFilters(defaults);
                userLookup.setSearch("");
                setSupplierSearch("");
                setVisible(defaultVisibility);
                apply(defaults);
              }}
            >
              <RotateCcw />
              重置
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={resource.refreshing}
              onClick={() => {
                resource.reload();
                onRefresh();
              }}
            >
              <RefreshCw />
              刷新
            </Button>
          </form>
          <div className="mt-3 flex flex-wrap gap-x-4 gap-y-2" aria-label="显示统计">
            {(Object.keys(seriesOptions) as (keyof typeof seriesOptions)[]).map((key) => (
              <Field key={key} orientation="horizontal" className="w-auto gap-2">
                <Checkbox
                  id={`overview-visible-${key}`}
                  checked={visible[key]}
                  onCheckedChange={(checked) =>
                    setVisible((current) => ({ ...current, [key]: checked === true }))
                  }
                />
                <FieldLabel htmlFor={`overview-visible-${key}`}>{seriesOptions[key]}</FieldLabel>
              </Field>
            ))}
          </div>
        </CardContent>
      </Card>
      <div
        className="grid grid-cols-2 gap-2 xl:grid-cols-4"
        aria-label="统计摘要"
        aria-busy={resource.refreshing}
      >
        {[
          {
            label: "请求量",
            value: summary?.request_count.toLocaleString() ?? "-",
            exact: summary?.request_count,
            detail: `成功 ${summary?.completed_requests ?? "-"} · 失败 ${summary?.failed_requests ?? "-"} · 进行中 ${summary ? summary.request_count - summary.completed_requests - summary.failed_requests : "-"}`,
          },
          {
            label: "总 Token",
            value: tokenCount(summary?.total_tokens),
            exact: summary?.total_tokens,
            detail: `输入 ${tokenCount(summary?.input_tokens)} · 输出 ${tokenCount(summary?.output_tokens)}`,
          },
          {
            label: "缓存命中率",
            value: percent(summary?.cache_rate),
            exact: summary?.cache_rate,
            detail: `读取 ${tokenCount(summary?.cached_tokens)} · 写入 ${tokenCount(summary?.cache_write_tokens)}`,
          },
          {
            label: "结算费用",
            value: usd(summary?.cost_nano_usd),
            exact: undefined,
            detail: `未计价 ${summary?.unpriced_requests ?? "-"} 个请求`,
          },
        ].map((item) => (
          <Card key={item.label} size="sm">
            <CardHeader className="gap-1 px-3">
              <CardDescription>{item.label}</CardDescription>
              <CardTitle
                className="break-all text-xl tabular-nums"
                title={item.exact?.toLocaleString("en-US")}
              >
                {item.value}
              </CardTitle>
              <CardDescription>{item.detail}</CardDescription>
            </CardHeader>
          </Card>
        ))}
      </div>
      <Card>
        <CardHeader className="gap-1">
          <CardTitle>用量统计 · {usageGroups[applied.group_by]}</CardTitle>
          <CardDescription>
            {applied.from.replace("T", " ")} 至 {applied.until.replace("T", " ")}
            （本地时间，结束时间不含）
            {chart.truncated ? " · 图表显示 Token 用量前 20 项，摘要汇总全部分组" : ""}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <ScrollArea className="w-full">
            <ChartContainer
              config={config}
              className="h-[420px] min-w-[320px] w-full aspect-auto"
              aria-label="用量统计组合图"
            >
              <ComposedChart
                accessibilityLayer
                data={chart.rows}
                margin={{ left: 12, right: 12, top: 12, bottom: 0 }}
              >
                <CartesianGrid vertical={false} />
                <XAxis
                  dataKey="label"
                  tickLine={false}
                  axisLine={false}
                  minTickGap={24}
                  tickFormatter={(value: string) =>
                    timeSeries
                      ? value.slice(5).replace("T", " ")
                      : value.length > 16
                        ? value.slice(0, 15) + "…"
                        : value
                  }
                />
                <YAxis yAxisId="tokens" hide width={0} domain={[0, "auto"]} />
                <YAxis
                  yAxisId="requests"
                  hide
                  width={0}
                  domain={[0, "auto"]}
                  allowDecimals={false}
                />
                <YAxis yAxisId="rate" hide width={0} domain={[0, 100]} />
                <YAxis yAxisId="cost" hide width={0} domain={[0, "auto"]} />
                <ChartTooltip
                  filterNull={false}
                  content={
                    <ChartTooltipContent
                      labelFormatter={(value, payload) =>
                        `${value}${payload[0]?.payload?.missing_token_requests ? ` · ${payload[0].payload.missing_token_requests} 个请求用量缺失` : ""}`
                      }
                      formatter={(value, name) =>
                        `${config[String(name)]?.label ?? name}：${value == null ? "-" : name === "cache_rate" ? percent(Number(value)) : name === "cost_usd" ? usd(Number(value) * 1e9) : name === "request_count" ? `${Number(value).toLocaleString("en-US")} 次` : `${tokenCount(Number(value))} Token`}`
                      }
                    />
                  }
                />
                <ChartLegend content={<ChartLegendContent className="flex-wrap" />} />
                {visible.models &&
                  chart.models.map((model) => (
                    <Bar
                      key={model.key}
                      dataKey={model.key}
                      legendType="none"
                      yAxisId="tokens"
                      stackId="models"
                      fill={`var(--color-${model.key})`}
                      fillOpacity={0.45}
                      maxBarSize={48}
                      isAnimationActive={false}
                    />
                  ))}
                {(
                  [
                    ["total_tokens", "tokens"],
                    ["request_count", "requests"],
                    ["cache_rate", "rate"],
                    ["cost_usd", "cost"],
                  ] as const
                )
                  .filter(([key]) => visible[key])
                  .map(([key, axis]) => (
                    <Line
                      key={key}
                      dataKey={key}
                      yAxisId={axis}
                      type="monotone"
                      stroke={`var(--color-${key})`}
                      strokeWidth={2.5}
                      dot={chart.rows.length <= 31 ? { r: 3 } : false}
                      connectNulls={false}
                      isAnimationActive={false}
                    />
                  ))}
              </ComposedChart>
            </ChartContainer>
            <ScrollBar orientation="horizontal" />
          </ScrollArea>
          {!chart.rows.length && (
            <CardDescription>
              {resource.loading
                ? "正在加载统计…"
                : resource.error && !resource.data
                  ? "统计尚未加载，请点击刷新重试"
                  : "当前筛选范围暂无用量记录"}
            </CardDescription>
          )}
        </CardContent>
      </Card>
    </>
  );
}
