"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { money, request, type Plan, type Subscription } from "@/lib/api";
import { orderStatus, useOrderClock, type CheckoutPreview, type Order } from "@/lib/orders";
import { useListResource } from "@/lib/pagination";
import { useResource } from "@/lib/resource";
import type { Dispatch, SetStateAction } from "react";
import { useId, useState } from "react";

export type CheckoutDialogProps = {
  subscriptionId: string;
  changeSubscription: (id: string) => void;
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
  const subscriptions = useResource<{ items: Subscription[] }>("/subscriptions/options", 0);
  const plans = useListResource<Plan>("/plans");
  const planPage = plans.pagination;
  const wallet = useResource<{ balance_usd: string }>("/wallet", 0);
  const actions = useActions();
  const [selected, setSelected] = useState<Plan>();
  const [subscriptionId, setSubscriptionId] = useState("");
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
    ((preview.subscription_id ?? "") !== subscriptionId ||
      coupon.trim().toUpperCase() !== (preview.coupon_code ?? "") ||
      payment !== preview.payment_method);
  const previewExpired = !!preview && preview.expires_at_ms <= now;
  const walletBalance = wallet.data ? `${money(wallet.data.balance_usd)} USD` : "—";
  const purchaseDisabled = !plans.ready || !subscriptions.ready || busy;
  function beginCheckout(plan: Plan) {
    setViewedId(undefined);
    setSelected(plan);
    setPreview(undefined);
    setOrder(undefined);
    setCoupon("");
    setPayment("wallet");
    const target = new URLSearchParams(window.location.search).get("subscription_id") ?? "";
    setSubscriptionId(target);
    void actions.run("preview", () => loadPreview(plan, "", "wallet", false, target), {
      success: "",
    });
  }
  async function loadPreview(
    plan: Plan,
    code: string,
    method: string,
    preserveDraft = false,
    targetId = subscriptionId,
  ) {
    const current = subscriptions.data?.items.find(
      (item) => item.id === targetId && item.provider_id === plan.provider_id,
    );
    if (targetId && !current) throw new Error("请选择当前平台的本人订阅");
    const value = await request<CheckoutPreview>("/checkout/preview", {
      method: "POST",
      body: {
        subscription_id: current?.id ?? null,
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
  function changeSubscription(next: string) {
    setSubscriptionId(next);
    setPreview(undefined);
    if (selected)
      void actions.run("preview", () => loadPreview(selected, coupon, payment, true, next), {
        success: "",
      });
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
    subscriptionId,
    changeSubscription,
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
    purchaseDisabled,
    beginCheckout,
    loadPreview,
    pay,
  } as const;
}
