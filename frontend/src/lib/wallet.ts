export const walletSources = { order_payment: "订单支付", system_adjustment: "系统操作" } as const;

/** Parse USD with integer arithmetic; never round an administrator's input. */
export function usdCents(value: string): number | null {
  const match = /^(\d+)(?:\.(\d{1,2}))?$/.exec(value.trim());
  if (!match) return null;
  const cents = BigInt(match[1]) * 100n + BigInt((match[2] ?? "").padEnd(2, "0"));
  return cents <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(cents) : null;
}
