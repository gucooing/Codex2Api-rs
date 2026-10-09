"use client";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useAuthorization } from "@/lib/authorization";
import Link from "next/link";

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

export function Authorization({ device = false }: { device?: boolean }) {
  const {
    actions,
    id,
    flow,
    error,
    setRevision,
    username,
    setUsername,
    password,
    setPassword,
    code,
    setCode,
    completed,
    cancelled,
    handleClick,
    handleSubmit2,
    handleSubmit3,
    handleClick4,
    handleClick5,
  } = useAuthorization({ device });
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
                onClick={() => handleClick()}
              >
                切换账户
              </Button>
            </>
          )}
          {!flow?.identity && !flow?.account_unavailable && !completed && !cancelled && (
            <form noValidate onSubmit={(event) => handleSubmit2(event)}>
              <FieldSet disabled={!flow || !!error || actions.isBusy("identify")}>
                <FieldGroup>
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
              <form noValidate onSubmit={(event) => handleSubmit3(event)}>
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
                      onClick={() => handleClick4()}
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
              onClick={() => handleClick5()}
            >
              取消授权
            </Button>
          )}
        </CardContent>
      </Card>
    </main>
  );
}
