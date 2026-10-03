// Official Desktop plan-names resource uses numbered personal Pro tiers.
export const subscriptionChoices = [
  { value: "free", label: "ChatGPT Free" },
  { value: "go", label: "ChatGPT Go" },
  { value: "plus", label: "ChatGPT Plus" },
  { value: "prolite", label: "ChatGPT Pro 100" },
  { value: "pro", label: "ChatGPT Pro 200" },
  { value: "promax", label: "ChatGPT Pro 500" },
  { value: "business", label: "ChatGPT Business" },
  { value: "enterprise", label: "ChatGPT Enterprise" },
  { value: "edu", label: "ChatGPT Edu" },
] as const;

export function subscriptionLabel(value: unknown): string {
  const alias: Record<string, string> = {
    team: "business",
    self_serve_business_prolite: "business",
    self_serve_business_usage_based: "business",
    ent26: "enterprise",
    enterprise_cbp_automation: "enterprise",
    enterprise_cbp_usage_based: "enterprise",
    education: "edu",
    edu_plus: "edu",
    edu_pro: "edu",
    k12: "edu",
    hc: "enterprise",
  };
  if (typeof value !== "string" || !value) return "未提供订阅";
  return (
    subscriptionChoices.find((choice) => choice.value === (alias[value] ?? value))?.label ??
    "未知订阅"
  );
}
