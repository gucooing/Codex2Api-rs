import type { BusinessField, Json, ModelRef, Plan, Model, OAuth } from "./api";

export const clientOwnedKeys = new Set([
  "cloud_preferences",
  "profile_page",
  "desktop_preferences",
  "payment_methods",
  "referral_tracking",
  "family",
  "family_notices",
  "notification_settings",
  "notifications",
  "pins",
  "projects",
  "auto_top_up",
  "verified_access",
  "automations",
  "installed_plugins",
  "voice",
  "onboarding",
  "browser_settings",
  "config_bundle",
]);
export function canEditConfig(key: string, readonly: boolean) {
  return (
    !readonly &&
    !clientOwnedKeys.has(key) &&
    !["subscription_entitlements", "quota", "models"].includes(key)
  );
}
export function allowedFields(key: string, fields: BusinessField[]) {
  return key === "user_settings" ? fields.filter((field) => field.path[0] === "flags") : fields;
}
export function modelKey(model: ModelRef) {
  return `${model.provider_id}/${model.model}`;
}
export function sameProviderModels(models: ModelRef[], provider: string) {
  return models.filter((model) => model.provider_id === provider);
}
export function planWrite(value: Plan) {
  return {
    name: value.name,
    provider_id: value.provider_id,
    model_access: value.model_access,
    models: value.models.map(({ provider_id, model }) => ({ provider_id, model })),
    free_model_access: value.free_model_access,
    free_models: value.free_models.map(({ provider_id, model }) => ({ provider_id, model })),
    free_access_enabled: value.free_access_enabled,
    primary_cost_limit_usd: value.primary_cost_limit_usd,
    weekly_cost_limit_usd: value.weekly_cost_limit_usd,
    free_primary_cost_limit_usd: value.free_primary_cost_limit_usd,
    free_weekly_cost_limit_usd: value.free_weekly_cost_limit_usd,
    spending_windows: value.spending_windows,
    free_spending_windows: value.free_spending_windows,
    enabled: value.enabled,
    revision: value.id ? value.revision : null,
  };
}
export function modelWrite(value: Model) {
  return {
    provider_id: value.provider_id,
    model: value.model,
    kind: value.kind,
    enabled: value.enabled,
    revision: value.revision,
    token_prices: value.kind === "text" ? value.token_prices : [],
    image_prices: value.kind === "image" ? value.image_prices : [],
  };
}
export function mergeOAuth(
  previous: OAuth | undefined,
  next: OAuth | { status: "pending" },
): OAuth {
  if (next.status === "complete") return next;
  if (!("state" in next) && previous?.status !== "pending")
    throw new Error("授权流程缺少上下文，请重新开始。");
  return { ...(previous?.status === "pending" ? previous : {}), ...next } as OAuth;
}
export function atPath(value: Json | undefined, path: string[]): Json | undefined {
  return path.reduce<Json | undefined>(
    (entry, key) =>
      entry && typeof entry === "object" && !Array.isArray(entry) ? entry[key] : undefined,
    value,
  );
}
export function withPath(value: Json, path: string[], next: Json | undefined): Json {
  if (!path.length) return next ?? null;
  const copy: { [key: string]: Json } =
    value && typeof value === "object" && !Array.isArray(value) ? { ...value } : {};
  const [key, ...rest] = path;
  if (["__proto__", "prototype", "constructor"].includes(key)) throw new Error("无效字段");
  if (!rest.length && next === undefined) delete copy[key];
  else copy[key] = withPath(copy[key] ?? null, rest, next);
  return copy;
}
export function localDate(value: string | null) {
  if (!value) return "";
  const date = new Date(value);
  return new Date(date.getTime() - date.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
}
export function billingLabel(status: string) {
  return (
    (
      {
        priced: "已计费",
        legacy: "历史未计费",
        pending: "待结算",
        unpriced: "模型未定价",
        missing_usage: "未上报完整用量",
        missing_resolution: "分辨率未知",
        not_charged: "未生成图片",
        invalid_usage: "用量异常",
        unsupported: "未定价",
        overflow: "金额超出范围",
      } as Record<string, string>
    )[status] ?? status
  );
}
export const scopes: Record<string, string> = {
  openid: "本人登录身份",
  profile: "本人资料",
  email: "本人邮箱",
  offline_access: "保持登录",
  "api.connectors.read": "本账户连接器读取",
  "api.connectors.invoke": "本账户连接器调用",
};
