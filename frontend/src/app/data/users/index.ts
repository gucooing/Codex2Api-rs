"use client";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import { request } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import { useListResource } from "@/lib/pagination";
import type { User, UserDetail, WalletEntry } from "@/lib/users";
import { usdCents } from "@/lib/wallet";
import type { Dispatch, SetStateAction } from "react";
import { useId, useState } from "react";

export type UserEditorDialogProps = {
  editing: (User & { password: string }) | undefined;
  actions: ReturnType<typeof useActions>;
  setEditing: Dispatch<SetStateAction<(User & { password: string }) | undefined>>;
  focus: ReturnType<typeof useDialogFocus>;
  resource: ReturnType<typeof useListResource<User>>;
  id: string;
};

export function useUserEditorDialog({
  editing,
  actions,
  setEditing,
  resource,
}: Pick<UserEditorDialogProps, "editing" | "actions" | "setEditing" | "resource">) {
  const handleSubmit = (e: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(e, "save-user", async () => {
      if (!editing || !resource.ready) throw new Error("请先加载用户资料");
      await request(editing.id ? `/users/${editing.id}` : "/users", {
        method: editing.id ? "PUT" : "POST",
        body: {
          username: editing.username,
          name: editing.name,
          email: editing.email,
          password: editing.password,
          enabled: editing.enabled,
          revision: editing.id ? editing.revision : null,
        },
      });
      setEditing(undefined);
      resource.reload();
    });
  return { handleSubmit } as const;
}

export function useUserRecords({ id }: { id: string }) {
  const resource = useResource<UserDetail>(`/users/${encodeURIComponent(id)}`);
  const wallet = useListResource<WalletEntry>(`/users/${encodeURIComponent(id)}/wallet-entries`);
  const pagination = wallet.pagination;
  useErrorToast(wallet.error);
  useErrorToast(resource.error);

  return { resource, pagination } as const;
}

export function useUsersPage() {
  const columns = useColumnVisibility(
    "users",
    ["用户名", "名称", "邮箱", "钱包（USD）", "状态", "操作"],
    ["用户名", "钱包（USD）", "操作"],
  );
  const [search, setSearch] = useState("");
  const [applied, setApplied] = useState("");
  const [editing, setEditing] = useState<User & { password: string }>();
  const [detailId, setDetailId] = useState<string>();
  const [adjusting, setAdjusting] = useState<{
    user: User;
    request_id: string;
    direction: "increase" | "decrease";
    amount: string;
    reason: string;
  }>();
  const before = adjusting ? usdCents(adjusting.user.wallet_balance_usd) : null;
  const amount = adjusting ? usdCents(adjusting.amount) : null;
  const delta = amount === null ? null : amount * (adjusting?.direction === "decrease" ? -1 : 1);
  const after = before === null || delta === null ? null : before + delta;
  const actions = useActions();
  const focus = useDialogFocus();
  const id = useId();
  const resource = useListResource<User>("/users", { search: applied });
  const pagination = resource.pagination;
  useErrorToast(resource.error);

  return {
    columns,
    search,
    setSearch,
    setApplied,
    editing,
    setEditing,
    detailId,
    setDetailId,
    adjusting,
    setAdjusting,
    before,
    amount,
    delta,
    after,
    actions,
    focus,
    id,
    resource,
    pagination,
  } as const;
}

export type WalletAdjustmentDialogProps = {
  adjusting:
    | {
        user: User;
        request_id: string;
        direction: "increase" | "decrease";
        amount: string;
        reason: string;
      }
    | undefined;
  actions: ReturnType<typeof useActions>;
  setAdjusting: Dispatch<
    SetStateAction<
      | {
          user: User;
          request_id: string;
          direction: "increase" | "decrease";
          amount: string;
          reason: string;
        }
      | undefined
    >
  >;
  resource: ReturnType<typeof useListResource<User>>;
  amount: number | null;
  delta: number | null;
  after: number | null;
  before: number | null;
  id: string;
};

export function useWalletAdjustmentDialog({
  adjusting,
  actions,
  setAdjusting,
  resource,
  amount,
  delta,
  after,
}: Pick<
  WalletAdjustmentDialogProps,
  "adjusting" | "actions" | "setAdjusting" | "resource" | "amount" | "delta" | "after"
>) {
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "adjust-wallet",
      async () => {
        if (
          !adjusting ||
          !resource.ready ||
          !amount ||
          delta === null ||
          after === null ||
          !Number.isSafeInteger(after) ||
          after < 0
        )
          throw new Error("请输入有效金额，减少金额不能超过余额");
        await request(`/users/${encodeURIComponent(adjusting.user.id)}/wallet-adjustments`, {
          method: "POST",
          body: {
            request_id: adjusting.request_id,
            amount_cents: delta,
            expected_revision: adjusting.user.revision,
            reason: adjusting.reason.trim() || null,
          },
        });
        setAdjusting(undefined);
        resource.reload();
      },
      "余额已调整",
    );
  return { handleSubmit } as const;
}
