/** Supplier tiers are stored with their documented display names: Free, Pro and Ultra. */
export function supplierSubscriptionLabel(value: unknown): string {
  return typeof value === "string" && value.trim() ? value : "未提供订阅";
}
