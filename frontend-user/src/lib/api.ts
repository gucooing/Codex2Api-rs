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
  id: string;
  name: string;
  provider_id: string;
  plan_type: string;
  sale_price_usd: string | null;
  duration_days: number;
  revision: number;
  model_access: string;
  models: { model: string }[];
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
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}
let csrf = "";
export function setSessionCsrf(value: string) {
  csrf = value;
}
export async function request<T>(
  path: string,
  options: { method?: "GET" | "POST"; body?: unknown; signal?: AbortSignal } = {},
): Promise<T> {
  const response = await fetch(`/user/api${path}`, {
    method: options.method ?? "GET",
    credentials: "same-origin",
    cache: "no-store",
    signal: options.signal,
    headers: {
      Accept: "application/json",
      ...(options.body !== undefined ? { "Content-Type": "application/json" } : {}),
      ...(options.method === "POST" && csrf ? { "X-CSRF-Token": csrf } : {}),
    },
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
  });
  const value = await response.json().catch(() => undefined);
  if (
    response.status === 401 &&
    !options.signal?.aborted &&
    path !== "/login" &&
    !path.startsWith("/oauth/") &&
    typeof window !== "undefined"
  )
    window.dispatchEvent(new Event("user-session-expired"));
  if (!response.ok || value === undefined)
    throw new ApiError(response.status, value?.error?.message ?? "暂时无法连接服务，请重试");
  return value;
}
export const money = (value: string | number) => `$${Number(value).toFixed(2)}`;
export const date = (value: string | null) =>
  value ? new Date(value).toLocaleString() : "长期有效";

export function requestId() {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (value) =>
    value.toString(16).padStart(2, "0"),
  ).join("");
}
