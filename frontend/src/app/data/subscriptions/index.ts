"use client";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import { query, request, type Consumer, type List, type PlanOption } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useQueryId, useResource } from "@/lib/hooks";
import { useListResource } from "@/lib/pagination";
import { useUserLookup, type UserOption } from "@/lib/user-lookup";
import type { UserDetail, UserSubscription } from "@/lib/users";
import type { Dispatch, SetStateAction } from "react";
import { useId, useState } from "react";

export function useSubscriptionDetail() {
  const platformPrefix = "/subscriptions";
  const id = useQueryId();
  const fieldId = useId();
  const resource = useResource<Consumer>(id ? `${platformPrefix}/${encodeURIComponent(id)}` : null);
  const [tab, setTab] = useState("settings");
  const [group, setGroup] = useState("account");
  const [refreshVersion, setRefreshVersion] = useState(0);
  useErrorToast(!id ? "缺少账户编号。" : undefined);
  useErrorToast(resource.error);
  const account = resource.data;

  return {
    platformPrefix,
    id,
    fieldId,
    resource,
    tab,
    setTab,
    group,
    setGroup,
    refreshVersion,
    setRefreshVersion,
    account,
  } as const;
}

export type SubscriptionEditorDialogProps = {
  editing: Edit | undefined;
  actions: ReturnType<typeof useActions>;
  setEditing: Dispatch<SetStateAction<Edit | undefined>>;
  focus: ReturnType<typeof useDialogFocus>;
  editorUser: ReturnType<typeof useResource<UserDetail>>;
  plans: ReturnType<typeof useResource<List<PlanOption>>>;
  resource: ReturnType<typeof useListResource<UserSubscription>>;
  id: string;
  editorLookup: ReturnType<typeof useUserLookup>;
  editorPlans: ReturnType<typeof useResource<List<PlanOption>>>;
};

export function useSubscriptionEditorDialog({
  editing,
  actions,
  setEditing,
  editorUser,
  plans,
  resource,
}: Pick<
  SubscriptionEditorDialogProps,
  "editing" | "actions" | "setEditing" | "editorUser" | "plans" | "resource"
>) {
  const handleSubmit = (e: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(e, "subscription", async () => {
      if (!editing || !editorUser.ready || !plans.ready || !resource.ready)
        throw new Error("请先加载订阅资料");
      if (!editing.user_id || !editing.plan_id) throw new Error("请选择用户和套餐");
      const plan = plans.data?.items.find((p) => p.id === editing.plan_id);
      const owned = await request<List<UserSubscription>>(
        `/subscriptions${query({ user_id: editing.user_id, provider_id: plan?.provider_id, include_expired: true })}`,
      );
      const existing = owned.items[0];
      const target = editing.id || existing?.virtual_account_id;
      await request(target ? `/subscriptions/${target}` : "/subscriptions", {
        method: target ? "PUT" : "POST",
        body: {
          user_id: editing.user_id,
          plan_id: editing.plan_id,
          expires_at: editing.expires_at ? new Date(editing.expires_at).toISOString() : null,
          enabled: editing.enabled,
          reissue: editing.reissue,
          revision: editing.revision ?? existing?.revision ?? null,
        },
      });
      setEditing(undefined);
      resource.reload();
    });
  return { handleSubmit } as const;
}

export type Edit = {
  reissue: boolean;
  id: string;
  user_id: string;
  provider_id: string;
  plan_id: string;
  expires_at: string;
  enabled: boolean;
  revision: number | null;
};

export const emptyFilters = { user_id: "", plan_id: "all", include_expired: false };

export function useSubscriptions({ initialUserId }: { initialUserId: string }) {
  const columns = useColumnVisibility(
    "subscriptions",
    ["用户", "平台", "套餐", "到期时间", "状态", "操作"],
    ["用户", "套餐", "操作"],
  );
  const [filters, setFilters] = useState({ ...emptyFilters, user_id: initialUserId });
  const [applied, setApplied] = useState({ ...emptyFilters, user_id: initialUserId });
  const [chosenUser, setChosenUser] = useState<UserOption | null>(
    initialUserId ? { id: initialUserId, username: "", name: "已选用户" } : null,
  );
  const userLookup = useUserLookup();
  const resource = useListResource<UserSubscription>("/subscriptions", {
    ...applied,
    plan_id: applied.plan_id === "all" ? "" : applied.plan_id,
  });
  const plans = useResource<List<PlanOption>>("/plans/options");
  const selectedUser = chosenUser;
  const [editing, setEditing] = useState<Edit>();
  const editorLookup = useUserLookup();
  const editorUser = useResource<UserDetail>(
    editing?.user_id ? `/users/${encodeURIComponent(editing.user_id)}` : null,
  );
  const editorPlans = useResource<List<PlanOption>>(
    `/plans/options${query({ provider_id: editing?.id ? editing.provider_id : "" })}`,
  );
  useErrorToast(editorUser.error);
  useErrorToast(editorPlans.error);
  const actions = useActions();
  const focus = useDialogFocus();
  const id = useId();
  const pagination = resource.pagination;
  useErrorToast(resource.error);
  useErrorToast(plans.error);

  return {
    columns,
    filters,
    setFilters,
    setApplied,
    setChosenUser,
    userLookup,
    resource,
    plans,
    selectedUser,
    editing,
    setEditing,
    editorLookup,
    editorUser,
    editorPlans,
    actions,
    focus,
    id,
    pagination,
  } as const;
}
