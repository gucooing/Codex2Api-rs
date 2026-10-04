"use client";
import { useEffect, useState } from "react";
export type Order = {
  user_id: string;
  username: string;
  user_name: string;
  id: string;
  provider_id: string;
  plan_id: string;
  plan_name: string;
  kind: "purchase" | "renew" | "upgrade";
  status: "pending" | "paid" | "cancelled" | "expired";
  previous_plan_name: string | null;
  unit_price_cents: number;
  duration_days: number;
  period_start_ms: number;
  period_end_ms: number;
  gross_cents: number;
  credit_cents: number;
  amount_cents: number;
  discount_cents: number;
  coupon_code: string | null;
  payment_method: "wallet";
  created_at_ms: number;
  quote_expires_at_ms: number;
  paid_at_ms: number | null;
  cancelled_at_ms: number | null;
  cancel_reason: string | null;
  balance_cents: number | null;
  pricing: {
    credit_periods: {
      starts_at_ms: number;
      ends_at_ms: number;
      unit_price_cents: number;
      duration_days: number;
      credit_cents: number;
    }[];
  };
};
export type OrderPage = { items: Order[]; total: number; page: number; limit: number };
export const orderKinds = { purchase: "购买", renew: "续订", upgrade: "升级" };
export const orderStatuses = {
  pending: "待支付",
  paid: "已支付",
  cancelled: "已取消",
  expired: "已过期",
};
export const cents = (value: number) => `$${(value / 100).toFixed(2)}`;
export const orderTime = (value: number | null) =>
  value === null ? "—" : new Date(value).toLocaleString();
export function orderStatus(order: Order, now: number) {
  return order.status === "pending" && order.quote_expires_at_ms <= now ? "expired" : order.status;
}
export function useOrderClock() {
  const [now, setNow] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  return now;
}
