"use client";
import { useEffect, useId, useState } from "react";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { FieldSet, FieldGroup, Field, FieldLabel } from "@/components/ui/field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useActions, useErrorToast } from "@/lib/actions";
import { scopes } from "@/lib/domain";

type Authorization = { request_id: string; csrf_token: string; client_name: string; scope: string };
async function oauthRequest<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api/oauth/chatgpt/oauth/authorize/${path}`, {
    method: body ? "POST" : "GET",
    credentials: "same-origin",
    cache: "no-store",
    headers: {
      Accept: "application/json",
      ...(body ? { "Content-Type": "application/json" } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const value = await response.json().catch(() => undefined);
  if (!response.ok || value == null)
    throw new Error(value?.error?.message ?? "授权信息尚未加载，请重试。");
  return value;
}
export default function AuthorizePage() {
  const id = useId();
  const actions = useActions();
  const [flow, setFlow] = useState<Authorization>();
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  useErrorToast(error);
  useEffect(() => {
    let cancelled = false;
    oauthRequest<Authorization>(`bootstrap${window.location.search}`)
      .then((value) => {
        if (!cancelled) {
          setFlow(value);
          setError("");
        }
      })
      .catch((reason) => {
        if (!cancelled) setError(reason.message);
      });
    return () => {
      cancelled = true;
    };
  }, [revision]);
  return (
    <main className="flex min-h-svh w-full items-center justify-center p-4">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle role="heading" aria-level={1}>
            登录虚拟账户
          </CardTitle>
          <CardDescription>
            授权 {flow?.client_name ?? "客户端"} 使用你的 ChatGPT 虚拟账户。
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {error && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setRevision((value) => value + 1)}
            >
              重新加载
            </Button>
          )}
          {flow && (
            <CardDescription>
              {flow.scope
                .split(/\s+/)
                .filter(Boolean)
                .map((scope) => scopes[scope] ?? scope)
                .join("、")}
            </CardDescription>
          )}
          <form
            noValidate
            onSubmit={(event) =>
              actions.submit(
                event,
                "consumer-authorize",
                async () => {
                  if (!flow) throw new Error("请先加载授权信息");
                  const result = await oauthRequest<{ redirect_uri: string }>("submit", {
                    request_id: flow.request_id,
                    csrf_token: flow.csrf_token,
                    username,
                    password,
                  });
                  setPassword("");
                  window.location.assign(result.redirect_uri);
                },
                "",
              )
            }
          >
            <FieldSet disabled={!flow || actions.isBusy("consumer-authorize")}>
              <FieldGroup className="gap-4">
                <Field>
                  <FieldLabel htmlFor={`${id}-username`}>虚拟账户用户名</FieldLabel>
                  <Input
                    id={`${id}-username`}
                    value={username}
                    onChange={(event) => setUsername(event.target.value)}
                    autoComplete="username"
                    required
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-password`}>密码</FieldLabel>
                  <Input
                    id={`${id}-password`}
                    type="password"
                    value={password}
                    onChange={(event) => setPassword(event.target.value)}
                    autoComplete="current-password"
                    required
                  />
                </Field>
                <Button type="submit" disabled={!flow || actions.isBusy("consumer-authorize")}>
                  {actions.isBusy("consumer-authorize") && <Spinner />}登录并授权
                </Button>
              </FieldGroup>
            </FieldSet>
          </form>
        </CardContent>
      </Card>
    </main>
  );
}
