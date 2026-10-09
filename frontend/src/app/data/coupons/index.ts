"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import type { PlanOption } from "@/lib/api";
import { request } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useListResource } from "@/lib/pagination";
import type { Dispatch, SetStateAction } from "react";
import { useId, useState } from "react";

export type Coupon = {
  id: string;
  code: string;
  name: string;
  enabled: boolean;
  discount_cents: number;
  minimum_cents: number;
  plan_id: string | null;
  starts_at_ms: number;
  ends_at_ms: number;
  max_uses: number | null;
  per_user_limit: number;
  revision: number;
  used_count: number;
  reserved_count: number;
};

export type Edit = {
  id?: string;
  code: string;
  name: string;
  enabled: boolean;
  discount: string;
  minimum: string;
  plan: string;
  starts: string;
  ends: string;
  max: string;
  perUser: string;
  revision: number | null;
};

export const localDate = (at: number) => {
  const d = new Date(at);
  return new Date(at - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
};

export function amount(text: string) {
  if (!/^\d+(\.\d{1,2})?$/.test(text.trim()))
    throw new Error("金额须为非负 USD 金额，最多两位小数");
  const [whole, fraction = ""] = text.trim().split(".");
  const result = Number(whole) * 100 + Number(fraction.padEnd(2, "0"));
  if (!Number.isSafeInteger(result) || result > 100000000) throw new Error("金额超出范围");
  return result;
}

export type CouponEditorDialogProps = {
  edit: Edit | undefined;
  busy: boolean;
  setEdit: Dispatch<SetStateAction<Edit | undefined>>;
  actions: ReturnType<typeof useActions>;
  resource: ReturnType<typeof useListResource<Coupon>>;
  plans: ReturnType<typeof useResource<{ items: PlanOption[] }>>;
  id: string;
  update: (patch: Partial<Edit>) => void;
};

export function useCouponEditorDialog({
  edit,
  setEdit,
  actions,
  resource,
  plans,
}: Pick<CouponEditorDialogProps, "edit" | "setEdit" | "actions" | "resource" | "plans">) {
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "save-coupon",
      async () => {
        if (!edit || !resource.ready || !plans.ready) throw new Error("请先加载优惠券和套餐");
        const starts = Date.parse(edit.starts),
          ends = Date.parse(edit.ends);
        const max = edit.max.trim() === "" ? null : Number(edit.max),
          perUser = Number(edit.perUser);
        if (!Number.isFinite(starts) || !Number.isFinite(ends) || ends <= starts)
          throw new Error("请选择有效的起止时间");
        if (
          (max !== null && (!Number.isSafeInteger(max) || max < 1)) ||
          !Number.isSafeInteger(perUser) ||
          perUser < 1
        )
          throw new Error("使用次数须为正整数");
        await request(edit.id ? `/coupons/${encodeURIComponent(edit.id)}` : "/coupons", {
          method: edit.id ? "PUT" : "POST",
          body: {
            code: edit.code,
            name: edit.name,
            enabled: edit.enabled,
            discount_cents: amount(edit.discount),
            minimum_cents: amount(edit.minimum),
            plan_id: edit.plan === "all" ? null : edit.plan,
            starts_at_ms: starts,
            ends_at_ms: ends,
            max_uses: max,
            per_user_limit: perUser,
            revision: edit.revision,
          },
        });
        setEdit(undefined);
        resource.reload();
      },
      "优惠券已保存",
    );
  return { handleSubmit } as const;
}

export function useCouponsPage() {
  const plans = useResource<{ items: PlanOption[] }>("/plans/options?paid_only=true");
  const [edit, setEdit] = useState<Edit>();
  const [search, setSearch] = useState("");
  const [applied, setApplied] = useState("");
  const actions = useActions();
  const id = useId();
  const resource = useListResource<Coupon>("/coupons", { search: applied });
  const pagination = resource.pagination;
  const update = (patch: Partial<Edit>) =>
    setEdit((value) => (value ? { ...value, ...patch } : value));
  const busy = actions.isBusy("save-coupon");
  useErrorToast(resource.error);
  useErrorToast(plans.error);

  return {
    plans,
    edit,
    setEdit,
    search,
    setSearch,
    setApplied,
    actions,
    id,
    resource,
    pagination,
    update,
    busy,
  } as const;
}
