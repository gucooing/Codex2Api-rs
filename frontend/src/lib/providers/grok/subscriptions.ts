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

export function subscriptionValue(value: string): string {
  const aliases: Record<string, string> = {
    GrokPro: "supergrok",
    SuperGrok: "supergrok",
    XBasic: "x_basic",
    XPremium: "x_premium",
    XPremiumPlus: "x_premium_plus",
    SuperGrokLite: "supergrok_lite",
    SuperGrokPlus: "supergrok_plus",
    SuperGrokHeavy: "supergrok_heavy",
    SuperGrokPro: "supergrok_heavy",
    premium: "x_premium",
    premium_plus: "x_premium_plus",
    supergrok_pro: "supergrok_heavy",
    Free: "free",
  };
  return aliases[value] ?? value;
}

export function subscriptionLabel(value: unknown): string {
  return typeof value === "string"
    ? (subscriptionChoices.find((c) => c.value === subscriptionValue(value))?.label ?? value)
    : "未提供订阅";
}

export function supplierSubscriptionLabel(value: unknown): string {
  return typeof value === "string" && value.trim() ? value : "未提供订阅";
}
