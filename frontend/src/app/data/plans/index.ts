"use client";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import {
  query,
  request,
  type List,
  type ModelOption,
  type Plan,
  type SpendingWindow,
  type SupplierTag,
} from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { planWrite } from "@/lib/domain";
import { useResource } from "@/lib/hooks";
import { useListResource } from "@/lib/pagination";
import {
  grokSubscriptionChoices,
  subscriptionChoices,
  subscriptionValue,
} from "@/lib/subscriptions";
import { useId, useState } from "react";

export const emptyPlan: Plan = {
  description: "",
  id: "",
  provider_id: "chatgpt",
  name: "",
  plan_type: "plus",
  model_access: "all",
  models: [],
  sale_price_usd: null,
  duration_days: 30,
  supplier_tag_id: null,
  allow_purchase: true,
  revision: 0,
  updated_at_ms: 0,
  spending_windows: [
    { duration_seconds: 604800, cost_limit_usd: null },
    { duration_seconds: 18000, cost_limit_usd: null },
  ],
};

export const windowLabel = (duration: number) =>
  duration === 18000 ? "5 小时" : duration === 604800 ? "7 天" : "30 天";

export function usePlanEditor({
  plan,
  ready,
  onSaved,
}: {
  plan: Plan;
  ready: boolean;
  onSaved: () => void;
}) {
  const [value, setValue] = useState(() => ({
    ...plan,
    description: plan.description ?? "",
    plan_type: subscriptionValue(plan.plan_type, plan.provider_id),
  }));
  const [search, setSearch] = useState("");
  const models = useResource<List<ModelOption>>(
    `/models/options${query({ provider_id: value.provider_id, search, plan_id: plan.id })}`,
    250,
  );
  const tags = useResource<List<SupplierTag>>(
    `/supplier-tags/options${query({ provider_id: value.provider_id })}`,
  );
  const actions = useActions();
  const focus = useDialogFocus();
  const id = useId();
  const busy = actions.isBusy("plan-save");
  const tiers = value.provider_id === "grok" ? grokSubscriptionChoices : subscriptionChoices;
  const update = <K extends keyof Plan>(key: K, next: Plan[K]) =>
    setValue((v) => ({ ...v, [key]: next }));
  const outer = value.spending_windows[0];
  const inner = value.spending_windows[1];
  const windowChange = (index: number, window: SpendingWindow) =>
    update(
      "spending_windows",
      value.spending_windows.map((w, i) => (i === index ? window : w)),
    );
  const choices = models.data?.items ?? [];
  useErrorToast(models.error);
  useErrorToast(tags.error);
  const handleSubmit = (e: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(e, "plan-save", async () => {
      if (!ready || !models.ready || !tags.ready) throw new Error("请先加载套餐、模型和号池资料");
      if (value.model_access === "selected" && !value.models.length)
        throw new Error("请至少选择一个模型");
      await request(plan.id ? `/plans/${plan.id}` : "/plans", {
        method: plan.id ? "PUT" : "POST",
        body: planWrite(value),
      });
      onSaved();
    });
  return {
    value,
    setValue,
    search,
    setSearch,
    models,
    tags,
    focus,
    id,
    busy,
    tiers,
    update,
    outer,
    inner,
    windowChange,
    choices,
    handleSubmit,
  } as const;
}

export function usePlansPage() {
  const columns = useColumnVisibility(
    "plans",
    ["套餐", "售价 / 有效期", "模型权限", "费用上限", "允许购买", "操作"],
    ["套餐", "售价 / 有效期", "操作"],
  );
  const [search, setSearch] = useState("");
  const [applied, setApplied] = useState("");
  const [editing, setEditing] = useState<Plan>();
  const actions = useActions();
  const id = useId();
  const resource = useListResource<Plan>("/plans", { search: applied });
  const pagination = resource.pagination;
  useErrorToast(resource.error);
  const handleClick = (plan: Plan) =>
    void actions.run(
      `delete-${plan.id}`,
      async () => {
        await request(`/plans/${plan.id}`, {
          method: "DELETE",
          body: { revision: plan.revision },
        });
        resource.reload();
      },
      { confirm: "删除此套餐？仍被使用的套餐无法删除。", danger: true },
    );
  return {
    columns,
    search,
    setSearch,
    setApplied,
    editing,
    setEditing,
    actions,
    id,
    resource,
    pagination,
    handleClick,
  } as const;
}
