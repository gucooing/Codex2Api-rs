export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(status === 409 ? `保存冲突：${message}。请重新加载后再保存。` : message);
  }
}
let csrf = "";
export function setCsrf(value: string) {
  csrf = value;
}
export async function request<T>(
  path: string,
  options: {
    method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
    body?: unknown;
    signal?: AbortSignal;
  } = {},
): Promise<T> {
  const method = options.method ?? "GET";
  const headers: Record<string, string> = { Accept: "application/json" };
  if (options.body !== undefined) headers["Content-Type"] = "application/json";
  if (method !== "GET" && csrf) headers["X-CSRF-Token"] = csrf;
  const body = options.body === undefined ? undefined : JSON.stringify(options.body);
  let response: Response;
  try {
    response = await fetch(`/admin/api${path}`, {
      method,
      credentials: "same-origin",
      cache: "no-store",
      headers,
      body,
      signal: options.signal,
    });
  } catch (error) {
    if (options.signal?.aborted) throw error;
    // A failed connection has no HTTP response status.
    throw new ApiError(0, "network_error", "无法连接后端服务，请检查网络后重试。");
  }
  let value: unknown;
  if (response.status !== 204) {
    try {
      value = await response.json();
    } catch (error) {
      if (options.signal?.aborted) throw error;
      if (response.ok)
        throw new ApiError(
          response.status,
          "invalid_response",
          "后端响应不完整或格式无效，请重试。",
        );
    }
  }
  if (!response.ok) {
    const envelope = value as
      { error?: { code?: string; message?: string } | string; message?: string } | undefined;
    const error = typeof envelope?.error === "object" ? envelope.error : undefined;
    if (
      response.status === 401 &&
      error?.code === "unauthorized" &&
      path !== "/login" &&
      path !== "/session"
    )
      window.dispatchEvent(new Event("admin-session-expired"));
    throw new ApiError(
      response.status,
      error?.code ?? "request_failed",
      error?.message ??
        envelope?.message ??
        (typeof envelope?.error === "string"
          ? envelope.error
          : response.status >= 500
            ? `后端服务暂时不可用 (${response.status})，请稍后重试。`
            : `请求失败 (${response.status})`),
    );
  }
  return value as T;
}
export function query(values: Record<string, string | number | boolean | null | undefined>) {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(values))
    if (value != null && value !== "") params.set(key, String(value));
  return params.size ? `?${params}` : "";
}
