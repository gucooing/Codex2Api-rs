"use client";
import { useState } from "react";
import Link from "next/link";
import { Bar, CartesianGrid, ComposedChart, Line, Area, AreaChart, XAxis, YAxis } from "recharts";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import { Progress } from "@/components/ui/progress";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { date, money, type Subscription } from "@/lib/api";
import { cents, orderKinds, orderStatuses, orderTime, type OrderPage } from "@/lib/orders";
import { tokens, usageCost, usageSeries, type UserUsage } from "@/lib/usage";
import { useResource } from "@/lib/resource";
import { useErrorToast } from "@/lib/actions";

const chartConfig = {
  request_count: { label: "请求数", color: "var(--chart-1)" },
  total_tokens: { label: "Token 用量", color: "var(--chart-2)" },
  cost_usd: { label: "已计费用量（USD）", color: "var(--chart-1)" },
};
export default function OverviewPage() {
  const [days, setDays] = useState("7");
  const subscriptions = useResource<{ items: Subscription[] }>("/subscriptions", 0);
  const wallet = useResource<{ balance_usd: string }>("/wallet", 0);
  const orders = useResource<OrderPage>("/orders?limit=5", 0);
  const usage = useResource<UserUsage>(
    `/usage?days=${days}&tz_offset=${new Date().getTimezoneOffset()}&limit=10`,
    0,
  );
  useErrorToast(subscriptions.error);
  useErrorToast(wallet.error);
  useErrorToast(orders.error);
  useErrorToast(usage.error);
  const refresh = () => {
    subscriptions.reload();
    wallet.reload();
    orders.reload();
    usage.reload();
  };
  const summary = usage.data?.summary;
  const series = usageSeries(usage.data);
  const cards = [
    {
      label: "钱包余额（USD）",
      value: wallet.data ? money(wallet.data.balance_usd) : "—",
      href: "/wallet/",
    },
    { label: "请求次数", value: summary?.request_count?.toLocaleString() ?? "—", href: "/usage/" },
    { label: "Token 用量", value: tokens(summary?.total_tokens), href: "/usage/" },
    { label: "已计费用量（USD）", value: usageCost(summary?.cost_nano_usd), href: "/usage/" },
  ];
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Select value={days} onValueChange={setDays}>
          <SelectTrigger className="w-36" aria-label="使用统计时间">
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            <SelectItem value="1">今天</SelectItem>
            <SelectItem value="7">最近 7 天</SelectItem>
            <SelectItem value="30">最近 30 天</SelectItem>
          </SelectContent>
        </Select>
        <Button variant="outline" onClick={refresh}>
          刷新
        </Button>
        <Button asChild>
          <Link href="/plans/">购买套餐</Link>
        </Button>
      </div>
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-4" aria-label="账户总览">
        {cards.map((card) => (
          <Button
            key={card.label}
            asChild
            variant="outline"
            className="h-16 flex-col items-start justify-center gap-1 px-3"
          >
            <Link href={card.href}>
              <span className="text-xs text-muted-foreground">{card.label}</span>
              <span className="text-lg font-semibold tabular-nums">{card.value}</span>
            </Link>
          </Button>
        ))}
      </div>
      <Card>
        <CardHeader>
          <CardTitle>当前订阅</CardTitle>
          <CardAction>
            <Button asChild variant="outline" size="sm">
              <Link href="/subscriptions/">管理订阅</Link>
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>平台</TableHead>
                <TableHead>套餐</TableHead>
                <TableHead>有效期</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>当期用量</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {subscriptions.data?.items.map((item) => (
                <TableRow key={item.id}>
                  <TableCell>
                    {item.provider_id === "chatgpt"
                      ? "ChatGPT"
                      : item.provider_id === "grok"
                        ? "Grok"
                        : item.provider_id}
                  </TableCell>
                  <TableCell>{item.plan_name}</TableCell>
                  <TableCell>{date(item.expires_at)}</TableCell>
                  <TableCell>
                    <Badge variant="secondary">
                      {!item.enabled ? "已停用" : item.plan_type === "free" ? "Free" : "有效"}
                    </Badge>
                  </TableCell>
                  <TableCell className="min-w-60">
                    <div className="space-y-3">
                      {item.spending_windows.map((window) => (
                        <div key={window.duration_seconds} className="space-y-1 text-xs">
                          <div className="flex flex-wrap justify-between gap-2">
                            <span>
                              {window.duration_seconds >= 86400
                                ? `${window.duration_seconds / 86400} 天`
                                : `${window.duration_seconds / 3600} 小时`}
                              ：{Number(window.used_usd).toFixed(4)} /{" "}
                              {window.limit_usd ?? "不限额"} USD
                            </span>
                            <span className="text-muted-foreground">
                              {window.reset_at
                                ? `${orderTime(window.reset_at * 1000)} 重置`
                                : "使用后开始计时"}
                            </span>
                          </div>
                          {window.used_percent !== null && (
                            <div className="flex items-center gap-2">
                              <Progress value={window.used_percent} />
                              <span>{window.used_percent}%</span>
                            </div>
                          )}
                          {window.unpriced_requests > 0 && (
                            <span className="text-muted-foreground">
                              另有 {window.unpriced_requests} 次请求尚无计费金额
                            </span>
                          )}
                        </div>
                      ))}
                      {item.spending_windows.length === 0 && (
                        <span className="text-xs text-muted-foreground">未设置额度窗口</span>
                      )}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {subscriptions.data?.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={5}>暂无订阅</TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
      <div className="grid gap-4 xl:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>请求与 Token 趋势</CardTitle>
            <CardDescription>
              {summary?.request_count === 0
                ? "所选时段内暂无使用记录"
                : "所选时段内本人的真实请求记录"}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <ChartContainer config={chartConfig} className="h-64 w-full">
              <ComposedChart accessibilityLayer data={series}>
                <CartesianGrid vertical={false} />
                <XAxis
                  dataKey="date"
                  tickFormatter={(value) => String(value).slice(5)}
                  tickLine={false}
                />
                <YAxis yAxisId="requests" allowDecimals={false} width={42} />
                <YAxis yAxisId="tokens" orientation="right" tickFormatter={tokens} width={55} />
                <ChartTooltip content={<ChartTooltipContent />} />
                <ChartLegend content={<ChartLegendContent />} />
                <Bar
                  yAxisId="requests"
                  dataKey="request_count"
                  fill="var(--color-request_count)"
                  radius={3}
                />
                <Line
                  yAxisId="tokens"
                  dataKey="total_tokens"
                  stroke="var(--color-total_tokens)"
                  dot={false}
                  connectNulls={false}
                />
              </ComposedChart>
            </ChartContainer>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>计费用量趋势</CardTitle>
            <CardDescription>按模型价格结算的套餐额度用量，与钱包购套餐支出分开</CardDescription>
          </CardHeader>
          <CardContent>
            <ChartContainer config={chartConfig} className="h-64 w-full">
              <AreaChart accessibilityLayer data={series}>
                <CartesianGrid vertical={false} />
                <XAxis
                  dataKey="date"
                  tickFormatter={(value) => String(value).slice(5)}
                  tickLine={false}
                />
                <YAxis tickFormatter={(value) => `$${Number(value).toFixed(3)}`} width={60} />
                <ChartTooltip
                  content={
                    <ChartTooltipContent formatter={(value) => `$${Number(value).toFixed(6)}`} />
                  }
                />
                <Area
                  dataKey="cost_usd"
                  fill="var(--color-cost_usd)"
                  fillOpacity={0.15}
                  stroke="var(--color-cost_usd)"
                  connectNulls={false}
                />
              </AreaChart>
            </ChartContainer>
          </CardContent>
        </Card>
      </div>
      <p className="text-xs text-muted-foreground">
        {summary
          ? `已完成 ${summary.completed_requests} 次，失败或中断 ${summary.failed_requests} 次；${summary.missing_token_requests} 次用量不完整，${summary.unpriced_requests} 次尚无计费金额。图表只汇总已知数值。`
          : "统计加载后显示请求和计费完整性。"}
      </p>
      <Card>
        <CardHeader>
          <CardTitle>最近订单</CardTitle>
          <CardAction>
            <Button asChild variant="outline" size="sm">
              <Link href="/orders/">查看全部</Link>
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>创建时间</TableHead>
                <TableHead>套餐</TableHead>
                <TableHead>类型</TableHead>
                <TableHead>应付（USD）</TableHead>
                <TableHead>状态</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {orders.data?.items.map((order) => (
                <TableRow key={order.id}>
                  <TableCell>{orderTime(order.created_at_ms)}</TableCell>
                  <TableCell>{order.plan_name}</TableCell>
                  <TableCell>{orderKinds[order.kind]}</TableCell>
                  <TableCell>{cents(order.amount_cents)}</TableCell>
                  <TableCell>
                    <Badge variant="secondary">{orderStatuses[order.status]}</Badge>
                  </TableCell>
                </TableRow>
              ))}
              {orders.data?.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={5}>暂无订单</TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </>
  );
}
