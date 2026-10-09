export type User = {
  id: string;
  username: string;
  name: string;
  email: string;
  enabled: boolean;
  created_at: string;
  wallet_balance_usd: string;
  revision: number;
};
export type Session = { user: User; csrf_token: string };
export type Subscription = {
  spending_windows: {
    duration_seconds: number;
    reset_at: number | null;
    used_percent: number | null;
    used_usd: string;
    limit_usd: string | null;
    remaining_usd: string | null;
    unpriced_requests: number;
  }[];
  current_price_cents: number | null;
  current_duration_days: number | null;
  id: string;
  provider_id: string;
  plan_id: string;
  plan_name: string;
  plan_type: string;
  expires_at: string | null;
  expired: boolean;
  enabled: boolean;
  revision: number;
};
export type Plan = {
  description: string;
  id: string;
  name: string;
  provider_id: string;
  plan_type: string;
  sale_price_usd: string | null;
  duration_days: number;
  revision: number;
  model_access: string;
  models: { provider_id: string; model: string; kind: string }[];
  spending_windows: { duration_seconds: number; cost_limit_usd: string | null }[];
};
export type Entry = {
  id: string;
  kind: "order_payment" | "system_adjustment";
  balance_before_cents: number;
  plan_name: string | null;
  amount_cents: number;
  balance_cents: number;
  duration_days: number | null;
  created_at: string;
  expires_at: string | null;
};
export type Device = {
  id: string;
  provider_id: string;
  user_agent: string;
  created_at: string;
  last_used_at: string | null;
};
export class ApiError extends Error {
  status: number;
  code: string;
  constructor(status: number, message: string, code = "request_failed") {
    super(message);
    this.status = status;
    this.code = code;
  }
}
let csrf = "";
let sessionRevision = 0;
let refreshing: Promise<void> | undefined;
export function setSessionCsrf(value: string) {
  csrf = value;
  sessionRevision++;
}
function currentCsrf() {
  return (
    (typeof document !== "undefined" &&
      document.cookie
        .split(";")
        .map((part) => part.trim())
        .find((part) => part.startsWith("c2a_user_csrf="))
        ?.slice("c2a_user_csrf=".length)) ||
    csrf
  );
}
export function refreshSession(): Promise<void> {
  if (!refreshing)
    refreshing = send("/session/refresh", { method: "POST" }, false)
      .then(() => {
        sessionRevision++;
      })
      .finally(() => {
        refreshing = undefined;
      });
  return refreshing;
}
type RequestOptions = { method?: "GET" | "POST"; body?: unknown; signal?: AbortSignal };
export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  return send<T>(path, options, true);
}
async function send<T>(path: string, options: RequestOptions, retry: boolean): Promise<T> {
  const revision = sessionRevision;
  const sessionCsrf = currentCsrf();
  const response = await fetch(`/user/api${path}`, {
    method: options.method ?? "GET",
    credentials: "same-origin",
    cache: "no-store",
    signal: options.signal,
    headers: {
      Accept: "application/json",
      ...(options.body !== undefined ? { "Content-Type": "application/json" } : {}),
      ...(options.method === "POST" && sessionCsrf ? { "X-CSRF-Token": sessionCsrf } : {}),
    },
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
  });
  const value = await response.json().catch(() => undefined);
  if (
    response.status === 401 &&
    value?.error?.code === "unauthorized" &&
    !options.signal?.aborted &&
    retry &&
    currentCsrf() === sessionCsrf &&
    path !== "/session/refresh" &&
    path !== "/login" &&
    !path.startsWith("/oauth/")
  ) {
    if (revision === sessionRevision) await refreshSession();
    options.signal?.throwIfAborted();
    return send<T>(path, options, false);
  }
  if (
    response.status === 401 &&
    value?.error?.code === "unauthorized" &&
    !options.signal?.aborted &&
    path !== "/login" &&
    currentCsrf() === sessionCsrf &&
    !path.startsWith("/oauth/") &&
    typeof window !== "undefined"
  )
    window.dispatchEvent(new Event("user-session-expired"));
  if (!response.ok || value === undefined)
    throw new ApiError(
      response.status,
      value?.error?.message ?? "暂时无法连接服务，请重试",
      value?.error?.code,
    );
  return value;
}
export const money = (value: string | number) => `$${Number(value).toFixed(2)}`;
export const date = (value: string | null) =>
  value ? new Date(value).toLocaleString() : "长期有效";
