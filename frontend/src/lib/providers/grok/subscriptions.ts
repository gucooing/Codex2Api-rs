export const subscriptionChoices = [
  { value: "free", label: "Free" },
  { value: "x_basic", label: "X Basic" },
  { value: "x_premium", label: "X Premium" },
  { value: "x_premium_plus", label: "X Premium+" },
  { value: "supergrok_lite", label: "SuperGrok Lite" },
  { value: "supergrok", label: "SuperGrok" },
  { value: "supergrok_plus", label: "SuperGrok Plus" },
  { value: "supergrok_heavy", label: "SuperGrok Heavy" },
] as const;

export function subscriptionLabel(value: unknown): string {
  return typeof value === "string"
    ? (subscriptionChoices.find((c) => c.value === value)?.label ?? value)
    : "未提供订阅";
}

export function supplierSubscriptionLabel(value: unknown): string {
  return typeof value === "string" && value.trim() ? value : "未提供订阅";
}
