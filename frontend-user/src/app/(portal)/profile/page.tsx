"use client";

import { useId, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";
import { useUserSession } from "@/components/user-shell";
import { request, date, money, type Session } from "@/lib/api";
import { useResource } from "@/lib/resource";
import { useActions, useErrorToast } from "@/lib/actions";

export default function ProfilePage() {
  const { session, signOut } = useUserSession();
  const profile = useResource<Session>("/session", 0);
  const user = profile.data?.user ?? session.user;
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const id = useId();
  const actions = useActions();
  useErrorToast(profile.error);
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
            <form
              noValidate
              onSubmit={(event) =>
                actions.submit(
                  event,
                  "password",
                  async () => {
                    await request("/password", {
                      method: "POST",
                      body: { current_password: currentPassword, new_password: newPassword },
                    });
                    signOut();
                    toast.success("密码已更新，请重新登录");
                  },
                  "",
                )
              }
            >
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
