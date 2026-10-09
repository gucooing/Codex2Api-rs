"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { query, request } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import { orderStatus, useOrderClock, type Order, type OrderPage } from "@/lib/orders";
import { usePageControls } from "@/lib/pagination";
import { useUserLookup, type UserOption } from "@/lib/user-lookup";
import type { Dispatch, SetStateAction } from "react";
import { useId, useState } from "react";

export type OrderDetailDialogProps = {
  selected: Order | undefined;
  busy: boolean;
  setSelected: Dispatch<SetStateAction<Order | undefined>>;
  effective: "expired" | "pending" | "paid" | "cancelled" | undefined;
  resource: ReturnType<typeof useResource<OrderPage>>;
  actions: ReturnType<typeof useActions>;
  act: (action: "cancel") => Promise<void>;
};

export function useOrderDetailDialog({
  actions,
  act,
}: Pick<OrderDetailDialogProps, "actions" | "act">) {
  const handleClick = () =>
    void actions.run("order-action", () => act("cancel"), { success: "订单已取消" });
  return { handleClick } as const;
}

export const emptyFilters = { search: "", status: "all", plan_id: "all", user_id: "" };

export function useOrdersPage() {
  const [filters, setFilters] = useState(emptyFilters);
  const [applied, setApplied] = useState(emptyFilters);
  const plans = useResource<{ items: { id: string; name: string; provider_id: string }[] }>(
    "/orders/plans",
  );
  const [selectedUser, setSelectedUser] = useState<UserOption | null>(null);
  const userLookup = useUserLookup();
  const [page, setPage] = useState(1);
  const [size, setSize] = useState(20);
  const [selected, setSelected] = useState<Order>();
  const id = useId();
  const now = useOrderClock();
  const resource = useResource<OrderPage>(
    `/orders${query({ search: applied.search, status: applied.status === "all" ? "" : applied.status, plan_id: applied.plan_id === "all" ? "" : applied.plan_id, user_id: applied.user_id, page, limit: size })}`,
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
  const columns = useColumnVisibility(
    "orders",
    ["用户", "订单 / 套餐", "应付", "状态", "创建时间", "操作"],
    ["用户", "订单 / 套餐", "应付", "操作"],
  );
  useErrorToast(resource.error);
  useErrorToast(plans.error);
  async function act(action: "cancel") {
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
    selectedUser,
    setSelectedUser,
    userLookup,
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
    columns,
    act,
  } as const;
}
