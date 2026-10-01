"use client";

import { useEffect, useId, useState } from "react";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { FieldSet, FieldGroup, Field, FieldLabel } from "@/components/ui/field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useActions, useErrorToast } from "@/lib/actions";

type DeviceFlow = { request_id: string; csrf_token: string };

async function deviceRequest<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api/oauth/chatgpt/oauth/device/${path}`, {
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
    throw new Error(value?.error?.message ?? "暂时无法授权，请重试。");
  return value;
}

export default function DevicePage() {
  const id = useId();
  const actions = useActions();
  const [flow, setFlow] = useState<DeviceFlow>();
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  const [userCode, setUserCode] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [authorized, setAuthorized] = useState(false);
  useErrorToast(error);
  useEffect(() => {
    let cancelled = false;
    deviceRequest<DeviceFlow>("bootstrap")
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
            设备登录授权
          </CardTitle>
          <CardDescription>
            {authorized
              ? "授权完成，请回到终端继续。"
              : "输入终端显示的设备码，仅授权你本人发起的登录。"}
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
          <form
            noValidate
            onSubmit={(event) =>
              actions.submit(
                event,
                "device-authorize",
                async () => {
                  if (!flow) throw new Error("请先加载授权信息");
                  await deviceRequest("approve", {
                    ...flow,
                    user_code: userCode,
                    username,
                    password,
                  });
                  setPassword("");
                  setAuthorized(true);
                },
                "授权完成",
              )
            }
          >
            <FieldSet disabled={!flow || authorized || actions.isBusy("device-authorize")}>
              <FieldGroup className="gap-4">
                <Field>
                  <FieldLabel htmlFor={`${id}-code`}>设备码</FieldLabel>
                  <Input
                    id={`${id}-code`}
                    value={userCode}
                    onChange={(event) => setUserCode(event.target.value)}
                    autoComplete="one-time-code"
                    maxLength={32}
                    required
                  />
                </Field>
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
                <Button
                  type="submit"
                  disabled={!flow || authorized || actions.isBusy("device-authorize")}
                >
                  {actions.isBusy("device-authorize") && <Spinner />}登录并授权
                </Button>
              </FieldGroup>
            </FieldSet>
          </form>
        </CardContent>
      </Card>
    </main>
  );
}
