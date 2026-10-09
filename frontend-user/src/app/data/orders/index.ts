"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { request } from "@/lib/api";
import { orderStatus, useOrderClock, type Order, type OrderPage } from "@/lib/orders";
import { usePageControls } from "@/lib/pagination";
import { useResource } from "@/lib/resource";
import type { Dispatch, SetStateAction } from "react";
import { useId, useState } from "react";

export type OrderDetailDialogProps = {
  selected: Order | undefined;
  busy: boolean;
  setSelected: Dispatch<SetStateAction<Order | undefined>>;
  effective: "pending" | "paid" | "cancelled" | "expired" | undefined;
  resource: ReturnType<typeof useResource<OrderPage>>;
  actions: ReturnType<typeof useActions>;
  act: (action: "pay" | "cancel") => Promise<void>;
};

export function useOrderDetailDialog({
  actions,
  act,
}: Pick<OrderDetailDialogProps, "actions" | "act">) {
  const handleClick = () =>
    void actions.run("order-action", () => act("cancel"), { success: "订单已取消" });
  const handleClick2 = () =>
    void actions.run("order-action", () => act("pay"), {
      success: "支付成功，订阅已更新",
    });
  return { handleClick, handleClick2 } as const;
}

export const emptyFilters = { search: "", status: "all", plan_id: "all" };

export function useOrdersPage() {
  const [filters, setFilters] = useState(emptyFilters);
  const [applied, setApplied] = useState(emptyFilters);
  const plans = useResource<{ items: { id: string; name: string; provider_id: string }[] }>(
    "/orders/plans",
    0,
  );
  const [page, setPage] = useState(1);
  const [size, setSize] = useState(20);
  const [selected, setSelected] = useState<Order>();
  const id = useId();
  const now = useOrderClock();
  const resource = useResource<OrderPage>(
    `/orders?search=${encodeURIComponent(applied.search)}&status=${applied.status === "all" ? "" : applied.status}&plan_id=${applied.plan_id === "all" ? "" : encodeURIComponent(applied.plan_id)}&page=${page}&limit=${size}`,
    0,
  );
  const pagination = usePageControls(
    page,
    resource.data?.total,
    setPage,
    size,
    !resource.data,
    setSize,
  );
  const actions = useActions();
  const effective = selected ? orderStatus(selected, now) : undefined;
  const busy = actions.isBusy("order-action");
  useErrorToast(resource.error);
  useErrorToast(plans.error);
  async function act(action: "pay" | "cancel") {
    if (!selected) return;
    try {
      setSelected(
        await request<Order>(`/orders/${encodeURIComponent(selected.id)}/${action}`, {
          method: "POST",
        }),
      );
    } catch (error) {
      const latest = await request<Order>(`/orders/${encodeURIComponent(selected.id)}`).catch(
        () => undefined,
      );
      if (latest) setSelected(latest);
      throw error;
    } finally {
      resource.reload();
    }
  }

  return {
    filters,
    setFilters,
    setApplied,
    plans,
    setPage,
    selected,
    setSelected,
    id,
    now,
    resource,
    pagination,
    actions,
    effective,
    busy,
    act,
  } as const;
}
