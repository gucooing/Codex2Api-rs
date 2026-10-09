"use client";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import { Progress } from "@/components/ui/progress";
import { UsagePageView } from "@/components/usage/usage-records";
import { date, money } from "@/lib/format";
import { useConsumerUsage } from "@/lib/platform-account";
import { tokenCount } from "@/lib/usage-display";
import { Inbox } from "lucide-react";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";

export function ConsumerUsage({ id }: { id: string }) {
  const { resource } = useConsumerUsage({ id });
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
