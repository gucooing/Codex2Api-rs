"use client";
import { useSubscriptionDetail } from "@/app/data/subscriptions";
import { ClientRecords } from "@/components/platform-account/client-records";
import { ConfigPanel } from "@/components/platform-account/config/config-panel";
import { Devices } from "@/components/platform-account/devices";
import { ConsumerResetCredits } from "@/components/platform-account/reset-credits";
import { Routing } from "@/components/platform-account/routing";
import { ConsumerUsage } from "@/components/platform-account/usage";
import { supplierChannel } from "@/components/providers";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Empty, EmptyDescription } from "@/components/ui/empty";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { date } from "@/lib/format";
import { ResourceRefreshContext } from "@/lib/hooks";
import { subscriptionLabel } from "@/lib/platform-account";
import Link from "next/link";
export function SubscriptionDetail() {
  const {
    platformPrefix,
    id,
    fieldId,
    resource,
    tab,
    setTab,
    group,
    setGroup,
    refreshVersion,
    setRefreshVersion,
    account,
  } = useSubscriptionDetail();
  if (!id)
    return (
      <Empty>
        <EmptyDescription>{"请选择用户订阅"}</EmptyDescription>
        <Button asChild variant="outline">
          <Link href={platformPrefix + "/"}>返回列表</Link>
        </Button>
      </Empty>
    );
  return (
    <ResourceRefreshContext value={refreshVersion}>
      <Tabs value={tab} onValueChange={setTab} className="min-w-0 gap-3">
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2">
          <div className="min-w-0 max-w-full flex-1 overflow-x-auto overflow-y-hidden pb-1">
            <TabsList variant="line" aria-label={"用户订阅详情"}>
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
              <Link
                href={
                  account?.user_id
                    ? `/subscriptions/?user_id=${encodeURIComponent(account.user_id)}`
                    : "/subscriptions/"
                }
              >
                返回列表
              </Link>
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
                    {supplierChannel(account?.provider_id).configGroups.map((item) => (
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
                      {
                        <div className="space-y-3">
                          <dl className="grid gap-3 text-sm sm:grid-cols-2">
                            <div>
                              <dt className="text-muted-foreground">用户名</dt>
                              <dd>{account?.username ?? "—"}</dd>
                            </div>
                            <div>
                              <dt className="text-muted-foreground">名称</dt>
                              <dd>{account?.name ?? "—"}</dd>
                            </div>
                            <div>
                              <dt className="text-muted-foreground">邮箱</dt>
                              <dd>{account?.email ?? "—"}</dd>
                            </div>
                            <div>
                              <dt className="text-muted-foreground">订阅套餐</dt>
                              <dd>{account?.plan_name ?? "—"}</dd>
                            </div>
                          </dl>
                          <Button asChild variant="outline">
                            <Link
                              href={
                                account?.user_id
                                  ? `/subscriptions/?user_id=${encodeURIComponent(account.user_id)}`
                                  : "/subscriptions/"
                              }
                            >
                              管理用户订阅
                            </Link>
                          </Button>
                        </div>
                      }
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
