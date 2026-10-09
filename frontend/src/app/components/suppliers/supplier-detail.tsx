"use client";
import { FingerprintEditor } from "@/app/components/suppliers/fingerprint-editor";
import { LocalUsage } from "@/app/components/suppliers/local-usage";
import { OAuthWizard } from "@/app/components/suppliers/oauth-wizard";
import { useSupplierDetail } from "@/app/data/suppliers";
import { SupplierTagEditor } from "@/components/forms/supplier-tags-form";
import { supplierChannel } from "@/components/providers";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Empty, EmptyDescription } from "@/components/ui/empty";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { date } from "@/lib/format";
import { supplierSubscriptionLabel as subscriptionLabel } from "@/lib/subscriptions";
import { supplierStatusLabel } from "@/lib/supplier-state";
import Link from "next/link";

export function SupplierDetail() {
  const {
    actions,
    id,
    resource,
    tab,
    setTab,
    relogin,
    setRelogin,
    account,
    profileRefresh,
    handleClick,
    handleClick2,
  } = useSupplierDetail();
  if (id === "")
    return (
      <Empty>
        <EmptyDescription>请选择供应账户</EmptyDescription>
        <Button asChild variant="outline">
          <Link href="/suppliers/">返回列表</Link>
        </Button>
      </Empty>
    );
  const ChannelOfficialData = supplierChannel(account?.provider_id).OfficialData;
  const ChannelModels = supplierChannel(account?.provider_id).Models;
  return (
    <>
      <div className="flex items-center justify-end gap-2">
        <CardDescription className="mr-auto truncate">{account?.email ?? "—"}</CardDescription>
        {profileRefresh && (
          <Button
            variant="outline"
            size="sm"
            disabled={!resource.ready || actions.isBusy("supplier-profile")}
            onClick={() => handleClick()}
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
                    variant="outline"
                    disabled={
                      !resource.ready || actions.isBusy("components\\suppliers.tsx:action:5")
                    }
                    onClick={() => handleClick2()}
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
