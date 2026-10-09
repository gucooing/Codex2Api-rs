"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { money, request, type Plan, type Subscription } from "@/lib/api";
import { orderStatus, useOrderClock, type CheckoutPreview, type Order } from "@/lib/orders";
import { useListResource } from "@/lib/pagination";
import { useResource } from "@/lib/resource";
import type { Dispatch, SetStateAction } from "react";
import { useId, useState } from "react";

export type CheckoutDialogProps = {
  selected: Plan | undefined;
  busy: boolean;
  setSelected: Dispatch<SetStateAction<Plan | undefined>>;
  stage: 1 | 3 | 2;
  status: "pending" | "paid" | "cancelled" | "expired" | undefined;
  detail: Order | CheckoutPreview | undefined;
  wallet: ReturnType<typeof useResource<{ balance_usd: string }>>;
  order: Order | undefined;
  preview: CheckoutPreview | undefined;
  walletBalance: string;
  id: string;
  payment: string;
  setPayment: Dispatch<SetStateAction<string>>;
  coupon: string;
  setCoupon: Dispatch<SetStateAction<string>>;
  plans: ReturnType<typeof useListResource<Plan>>;
  subscriptions: ReturnType<typeof useResource<{ items: Subscription[] }>>;
  actions: ReturnType<typeof useActions>;
  loadPreview: (plan: Plan, code: string, method: string, preserveDraft?: boolean) => Promise<void>;
  dirty: boolean;
  previewExpired: boolean;
  setOrder: Dispatch<SetStateAction<Order | undefined>>;
  refresh: () => void;
  pay: () => Promise<void>;
};

export function useCheckoutDialog({
  selected,
  order,
  preview,
  payment,
  coupon,
  actions,
  loadPreview,
  setOrder,
  refresh,
  pay,
}: Pick<
  CheckoutDialogProps,
  | "selected"
  | "order"
  | "preview"
  | "payment"
  | "coupon"
  | "actions"
  | "loadPreview"
  | "setOrder"
  | "refresh"
  | "pay"
>) {
  const handleClick = () => {
    if (selected && coupon.trim())
      void actions.run("apply-coupon", () => loadPreview(selected, coupon, payment), {
        success: "优惠码已使用",
      });
  };
  const handleClick2 = () => {
    if (selected)
      void actions.run("remove-coupon", () => loadPreview(selected, "", payment), {
        success: "优惠码已移除",
      });
  };
  const handleClick3 = () => {
    if (selected)
      void actions.run(
        "preview",
        () => loadPreview(selected, preview?.coupon_code ?? "", payment, true),
        { success: "预览已更新" },
      );
  };
  const handleClick4 = () =>
    void actions.run(
      "confirm-order",
      async () => {
        if (!preview) return;
        try {
          setOrder(
            await request<Order>("/orders", {
              method: "POST",
              body: { preview_token: preview.preview_token },
            }),
          );
        } finally {
          refresh();
        }
      },
      { success: "订单已创建，请确认支付" },
    );
  const handleClick5 = () =>
    void actions.run(
      "cancel-order",
      async () => {
        if (order)
          setOrder(
            await request<Order>(`/orders/${encodeURIComponent(order.id)}/cancel`, {
              method: "POST",
            }),
          );
      },
      { success: "订单已取消" },
    );
  const handleClick6 = () =>
    void actions.run("pay-order", pay, { success: "支付成功，订阅已更新" });
  return {
    handleClick,
    handleClick2,
    handleClick3,
    handleClick4,
    handleClick5,
    handleClick6,
  } as const;
}

export function usePlansPage() {
  const subscriptions = useResource<{ items: Subscription[] }>("/subscriptions", 0);
  const plans = useListResource<Plan>("/plans");
  const planPage = plans.pagination;
  const wallet = useResource<{ balance_usd: string }>("/wallet", 0);
  const actions = useActions();
  const [selected, setSelected] = useState<Plan>();
  const [viewedId, setViewedId] = useState<string>();
  const viewed = plans.data?.items.find((plan) => plan.id === viewedId);
  const [preview, setPreview] = useState<CheckoutPreview>();
  const [order, setOrder] = useState<Order>();
  const [coupon, setCoupon] = useState("");
  const [payment, setPayment] = useState("wallet");
  const id = useId();
  const now = useOrderClock();
  const refresh = () => {
    subscriptions.reload();
    plans.reload();
    wallet.reload();
  };
  useErrorToast(subscriptions.error);
  useErrorToast(plans.error);
  useErrorToast(wallet.error);
  const status = order ? orderStatus(order, now) : undefined;
  const stage = !order ? 1 : status === "paid" ? 3 : 2;
  const busy = [
    "preview",
    "apply-coupon",
    "remove-coupon",
    "confirm-order",
    "pay-order",
    "cancel-order",
  ].some(actions.isBusy);
  const detail = order ?? preview;
  const dirty =
    !!preview &&
    (coupon.trim().toUpperCase() !== (preview.coupon_code ?? "") ||
      payment !== preview.payment_method);
  const previewExpired = !!preview && preview.expires_at_ms <= now;
  const walletBalance = wallet.data ? `${money(wallet.data.balance_usd)} USD` : "—";
  function purchaseStatus(plan: Plan) {
    const current = subscriptions.data?.items.find((item) => item.provider_id === plan.provider_id);
    const paid =
      !!current &&
      current.plan_type !== "free" &&
      !current.expired &&
      (current.expires_at === null || Date.parse(current.expires_at) > now);
    const same = paid && current.plan_id === plan.id;
    const lower =
      paid &&
      !same &&
      current.current_price_cents != null &&
      current.current_duration_days != null &&
      plan.sale_price_usd !== null &&
      Number(plan.sale_price_usd) * 100 * current.current_duration_days <=
        current.current_price_cents * plan.duration_days;
    const unavailable = !!current && (!current.enabled || (paid && current.expires_at === null));
    return {
      disabled:
        !plans.ready ||
        !subscriptions.ready ||
        plan.sale_price_usd === null ||
        unavailable ||
        lower ||
        busy,
      label: unavailable
        ? "请联系管理员"
        : lower
          ? "到期后可更换"
          : same
            ? "续订"
            : paid
              ? "升级套餐"
              : "下单",
    };
  }
  function beginCheckout(plan: Plan) {
    setViewedId(undefined);
    setSelected(plan);
    setPreview(undefined);
    setOrder(undefined);
    setCoupon("");
    setPayment("wallet");
    void actions.run("preview", () => loadPreview(plan, "", "wallet"), { success: "" });
  }
  async function loadPreview(plan: Plan, code: string, method: string, preserveDraft = false) {
    const current = subscriptions.data?.items.find((item) => item.provider_id === plan.provider_id);
    const value = await request<CheckoutPreview>("/checkout/preview", {
      method: "POST",
      body: {
        plan_id: plan.id,
        plan_revision: plan.revision,
        subscription_revision: current?.revision ?? null,
        payment_method: method,
        coupon_code: code.trim(),
      },
    });
    setSelected(plan);
    setPreview(value);
    if (!preserveDraft) setCoupon(value.coupon_code ?? "");
  }
  async function pay() {
    if (!order) return;
    try {
      setOrder(
        await request<Order>(`/orders/${encodeURIComponent(order.id)}/pay`, { method: "POST" }),
      );
    } catch (error) {
      const latest = await request<Order>(`/orders/${encodeURIComponent(order.id)}`).catch(
        () => undefined,
      );
      if (latest) setOrder(latest);
      throw error;
    } finally {
      refresh();
    }
  }

  return {
    subscriptions,
    plans,
    planPage,
    wallet,
    actions,
    selected,
    setSelected,
    viewedId,
    setViewedId,
    viewed,
    preview,
    order,
    setOrder,
    coupon,
    setCoupon,
    payment,
    setPayment,
    id,
    refresh,
    status,
    stage,
    busy,
    detail,
    dirty,
    previewExpired,
    walletBalance,
    purchaseStatus,
    beginCheckout,
    loadPreview,
    pay,
  } as const;
}
