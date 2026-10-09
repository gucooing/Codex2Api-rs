"use client";
import { useErrorToast } from "@/lib/actions";
import { query } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import { usePageControls } from "@/lib/pagination";
import { useUserLookup, type UserOption } from "@/lib/user-lookup";
import type { AdminWalletEntry, WalletEntryPage } from "@/lib/users";
import { useId, useState } from "react";

export const defaults = { user_id: "", kind: "all", from: "", until: "" };

export type Applied = {
  user_id: string;
  kind: string;
  from_ms: number | null;
  until_ms: number | null;
};

export const emptyQuery: Applied = { user_id: "", kind: "", from_ms: null, until_ms: null };

export function useWalletEntriesPage() {
  const id = useId();
  const [filters, setFilters] = useState(defaults);
  const [applied, setApplied] = useState(emptyQuery);
  const [selectedUser, setSelectedUser] = useState<UserOption | null>(null);
  const [selected, setSelected] = useState<AdminWalletEntry>();
  const [page, setPage] = useState(1);
  const [size, setSize] = useState(20);
  const userLookup = useUserLookup();
  const resource = useResource<WalletEntryPage>(
    `/wallet-entries${query({ ...applied, page, limit: size })}`,
  );
  const pagination = usePageControls(
    page,
    resource.data?.total,
    setPage,
    size,
    !resource.data,
    setSize,
  );
  const columns = useColumnVisibility(
    "wallet-entries",
    ["时间", "用户", "来源", "变动", "前余额", "后余额", "操作人", "原因", "操作"],
    ["用户", "变动", "后余额", "操作"],
  );
  useErrorToast(resource.error);

  return {
    id,
    filters,
    setFilters,
    setApplied,
    selectedUser,
    setSelectedUser,
    selected,
    setSelected,
    setPage,
    userLookup,
    resource,
    pagination,
    columns,
  } as const;
}
