"use client";
import { useEffect, useId, useState } from "react";
import Link from "next/link";
import { request } from "@/lib/api";
import { useActions, useErrorToast } from "@/lib/actions";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel, FieldSet, FieldGroup } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";

const scopeLabels: Record<string, string> = {
  openid: "本人登录身份",
  profile: "本人资料",
  email: "本人邮箱",
  offline_access: "保持登录",
  "grok-cli:access": "使用 Grok Build",
  "api:access": "调用本账户模型",
  "conversations:read": "读取本账户会话",
  "conversations:write": "保存本账户会话",
  "workspaces:read": "读取本账户工作区",
  "workspaces:write": "保存本账户工作区",
  "api.connectors.read": "读取本账户连接器",
  "api.connectors.invoke": "调用本账户连接器",
};
type Identity = {
  account_id: string;
  username: string;
  name: string;
  email: string;
  provider_id: string;
};
type Flow = {
  request_id: string;
  csrf_token: string;
  client_name: string;
  provider_id: string;
  scope: string;
  identity: Identity | null;
  user: { name: string; username: string; email: string } | null;
  account_unavailable: boolean;
};
export function Authorization({ device = false }: { device?: boolean }) {
  const actions = useActions();
  const id = useId();
  const prefix = `/oauth/${device ? "device" : "authorize"}`;
  const [flow, setFlow] = useState<Flow>();
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  const [kind, setKind] = useState("user");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [completed, setCompleted] = useState(false);
  const [cancelled, setCancelled] = useState(false);
  useErrorToast(error);
  useEffect(() => {
    const controller = new AbortController();
    request<Flow>(`${prefix}/bootstrap${window.location.search}`, {
      signal: controller.signal,
    })
      .then((value) => {
        if (!controller.signal.aborted) {
          setFlow(value);
          if (device)
            setCode(
              (previous) =>
                previous || new URLSearchParams(window.location.search).get("user_code") || "",
            );
          setError("");
        }
      })
      .catch((reason) => {
        if (!controller.signal.aborted)
          setError(reason instanceof Error ? reason.message : "无法读取授权请求");
      });
    return () => controller.abort();
  }, [prefix, device, revision]);
  const reset = async () => {
    if (!flow) return;
    await request(`${prefix}/reset`, {
      method: "POST",
      body: { request_id: flow.request_id, csrf_token: flow.csrf_token },
    });
    setFlow({ ...flow, identity: null, user: null, account_unavailable: false });
    setPassword("");
  };
  const cancel = async () => {
    if (!flow) return;
    const result = await request<{ redirect_uri?: string; cancelled?: boolean }>(
      `${prefix}/cancel`,
      {
        method: "POST",
        body: { request_id: flow.request_id, csrf_token: flow.csrf_token },
      },
    );
    setPassword("");
    setFlow(undefined);
    if (result.redirect_uri) window.location.assign(result.redirect_uri);
    else if (result.cancelled) setCancelled(true);
    else throw new Error("取消授权失败，请重新发起登录");
  };
  return (
    <main className="flex min-h-svh items-center justify-center p-4">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle>
            {cancelled ? "已取消授权" : completed ? "授权完成" : "确认客户端登录"}
          </CardTitle>
          <CardDescription>
            {cancelled
              ? "没有授权客户端登录，可关闭此页面。"
              : completed
                ? "请回到客户端继续。"
                : `授权 ${flow?.client_name ?? "Codex"} 登录你的 ${flow?.provider_id === "grok" ? "Grok" : "ChatGPT"} 平台账户。`}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {error && (
            <Button variant="outline" onClick={() => setRevision((v) => v + 1)}>
              重新加载
            </Button>
          )}
          {flow?.account_unavailable && (
            <>
              <dl className="space-y-2 break-words text-sm">
                <div>
                  <dt>当前用户</dt>
                  <dd>
                    {flow.user?.name}（{flow.user?.username}）
                  </dd>
                </div>
                <div>
                  <dt>邮箱</dt>
                  <dd>{flow.user?.email}</dd>
                </div>
              </dl>
              <p className="text-sm text-muted-foreground">该平台账户已停用，请联系管理员。</p>
              <Button asChild>
                <Link href="/">进入用户中心</Link>
              </Button>
              <Button
                variant="outline"
                disabled={actions.isBusy("switch")}
                onClick={() => void actions.run("switch", reset, { success: "" })}
              >
                切换账户
              </Button>
            </>
          )}
          {!flow?.identity && !flow?.account_unavailable && !completed && !cancelled && (
            <form
              noValidate
              onSubmit={(event) =>
                actions.submit(
                  event,
                  "identify",
                  async () => {
                    if (!flow) throw new Error("请先加载授权请求");
                    const result = await request<{ identity: Identity }>(`${prefix}/identify`, {
                      method: "POST",
                      body: {
                        request_id: flow.request_id,
                        csrf_token: flow.csrf_token,
                        kind,
                        username,
                        password,
                      },
                    });
                    setPassword("");
                    setFlow({ ...flow, identity: result.identity });
                  },
                  "",
                )
              }
            >
              <FieldSet disabled={!flow || !!error || actions.isBusy("identify")}>
                <FieldGroup>
                  <Field>
                    <FieldLabel htmlFor={`${id}-kind`}>登录身份</FieldLabel>
                    <Select value={kind} onValueChange={setKind}>
                      <SelectTrigger id={`${id}-kind`}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        <SelectItem value="user">用户账户</SelectItem>
                        <SelectItem value="virtual">独立虚拟账户</SelectItem>
                      </SelectContent>
                    </Select>
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-username`}>用户名</FieldLabel>
                    <Input
                      id={`${id}-username`}
                      autoComplete="username"
                      required
                      value={username}
                      onChange={(e) => setUsername(e.target.value)}
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-password`}>密码</FieldLabel>
                    <Input
                      id={`${id}-password`}
                      type="password"
                      autoComplete="current-password"
                      required
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                    />
                  </Field>
                  <CardDescription>登录仅用于本次授权链接，验证后还需确认身份。</CardDescription>
                  <Button type="submit">{actions.isBusy("identify") && <Spinner />}验证身份</Button>
                </FieldGroup>
              </FieldSet>
            </form>
          )}
          {flow?.identity && !completed && (
            <>
              <dl className="space-y-2 break-words text-sm">
                <div>
                  <dt className="text-muted-foreground">用户名</dt>
                  <dd>{flow.identity.username}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">名称</dt>
                  <dd>{flow.identity.name}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">邮箱</dt>
                  <dd>{flow.identity.email}</dd>
                </div>
              </dl>
              <CardDescription>
                授权范围：
                {flow.scope
                  .split(/\s+/)
                  .filter(Boolean)
                  .map((scope) => scopeLabels[scope] ?? scope)
                  .join("、")}
                。
              </CardDescription>
              <form
                noValidate
                onSubmit={(event) =>
                  actions.submit(
                    event,
                    "approve",
                    async () => {
                      if (!flow.identity) return;
                      const result = await request<{ redirect_uri?: string; authorized?: boolean }>(
                        `${prefix}/approve`,
                        {
                          method: "POST",
                          body: {
                            request_id: flow.request_id,
                            csrf_token: flow.csrf_token,
                            account_id: flow.identity.account_id,
                            confirmed: true,
                            ...(device ? { user_code: code } : {}),
                          },
                        },
                      );
                      if (device && result.authorized) setCompleted(true);
                      else if (result.redirect_uri) window.location.assign(result.redirect_uri);
                      else throw new Error("授权响应不完整，请重新发起登录");
                    },
                    "",
                  )
                }
              >
                <FieldSet disabled={actions.isBusy("approve") || !!error}>
                  <FieldGroup>
                    {device && (
                      <Field>
                        <FieldLabel htmlFor={`${id}-code`}>设备码</FieldLabel>
                        <Input
                          id={`${id}-code`}
                          autoComplete="one-time-code"
                          required
                          maxLength={32}
                          value={code}
                          onChange={(e) => setCode(e.target.value)}
                        />
                      </Field>
                    )}
                    <Button type="submit">
                      {actions.isBusy("approve") && <Spinner />}确认登录
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      disabled={actions.isBusy("switch")}
                      onClick={() => void actions.run("switch", reset, { success: "" })}
                    >
                      切换账户
                    </Button>
                  </FieldGroup>
                </FieldSet>
              </form>
            </>
          )}
          {!completed && !cancelled && (
            <Button
              type="button"
              variant="ghost"
              disabled={!flow || actions.running.size > 0}
              onClick={() => void actions.run("cancel-authorization", cancel, { success: "" })}
            >
              取消授权
            </Button>
          )}
        </CardContent>
      </Card>
    </main>
  );
}
