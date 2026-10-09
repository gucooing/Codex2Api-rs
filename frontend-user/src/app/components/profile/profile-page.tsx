"use client";
import { useProfilePage } from "@/app/data/profile";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { date, money } from "@/lib/api";

export default function ProfilePage() {
  const {
    profile,
    user,
    currentPassword,
    setCurrentPassword,
    newPassword,
    setNewPassword,
    id,
    actions,
    handleSubmit,
  } = useProfilePage();
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" onClick={profile.reload}>
          刷新
        </Button>
      </div>
      <div className="grid items-start gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>基本信息</CardTitle>
          </CardHeader>
          <CardContent>
            <dl className="grid gap-4 text-sm sm:grid-cols-2">
              <div className="space-y-1">
                <dt className="text-muted-foreground">用户名</dt>
                <dd className="break-words">{user.username}</dd>
              </div>
              <div className="space-y-1">
                <dt className="text-muted-foreground">名称</dt>
                <dd className="break-words">{user.name}</dd>
              </div>
              <div className="space-y-1">
                <dt className="text-muted-foreground">邮箱</dt>
                <dd className="break-all">{user.email}</dd>
              </div>
              <div className="space-y-1">
                <dt className="text-muted-foreground">账户状态</dt>
                <dd>
                  <Badge variant="secondary">{user.enabled ? "正常" : "已停用"}</Badge>
                </dd>
              </div>
              <div className="space-y-1">
                <dt className="text-muted-foreground">创建时间</dt>
                <dd>{date(user.created_at)}</dd>
              </div>
              <div className="space-y-1">
                <dt className="text-muted-foreground">钱包余额（USD）</dt>
                <dd className="tabular-nums">{money(user.wallet_balance_usd)}</dd>
              </div>
            </dl>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>修改密码</CardTitle>
            <CardDescription>修改成功后需要重新登录。</CardDescription>
          </CardHeader>
          <CardContent>
            <form noValidate onSubmit={(event) => handleSubmit(event)}>
              <FieldSet disabled={!profile.ready || actions.isBusy("password")}>
                <FieldGroup>
                  <Field>
                    <FieldLabel htmlFor={`${id}-current`}>当前密码</FieldLabel>
                    <Input
                      id={`${id}-current`}
                      type="password"
                      autoComplete="current-password"
                      required
                      value={currentPassword}
                      onChange={(event) => setCurrentPassword(event.target.value)}
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-new`}>新密码</FieldLabel>
                    <Input
                      id={`${id}-new`}
                      type="password"
                      autoComplete="new-password"
                      required
                      value={newPassword}
                      onChange={(event) => setNewPassword(event.target.value)}
                    />
                  </Field>
                  <Button type="submit">{actions.isBusy("password") && <Spinner />}修改密码</Button>
                </FieldGroup>
              </FieldSet>
            </form>
          </CardContent>
        </Card>
      </div>
    </>
  );
}
