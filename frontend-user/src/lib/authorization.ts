"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { request } from "@/lib/api";
import { useEffect, useId, useState } from "react";

export type Identity = {
  account_id: string;
  username: string;
  name: string;
  email: string;
  provider_id: string;
};

export type Flow = {
  request_id: string;
  csrf_token: string;
  client_name: string;
  provider_id: string;
  scope: string;
  identity: Identity | null;
  user: { name: string; username: string; email: string } | null;
  account_unavailable: boolean;
};

export function useAuthorization({ device = false }: { device?: boolean }) {
  const actions = useActions();
  const id = useId();
  const prefix = `/oauth/${device ? "device" : "authorize"}`;
  const [flow, setFlow] = useState<Flow>();
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
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
  const handleClick = () => void actions.run("switch", reset, { success: "" });
  const handleSubmit2 = (event: React.SubmitEvent<HTMLFormElement>) =>
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
            username,
            password,
          },
        });
        setPassword("");
        setFlow({ ...flow, identity: result.identity });
      },
      "",
    );
  const handleSubmit3 = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "approve",
      async () => {
        if (!flow?.identity) throw new Error("请先验证登录身份");
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
    );
  const handleClick4 = () => void actions.run("switch", reset, { success: "" });
  const handleClick5 = () => void actions.run("cancel-authorization", cancel, { success: "" });
  return {
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
  } as const;
}
