"use client";
import { useState } from "react";
import { query } from "./api";
import { useResource } from "./hooks";
import { useErrorToast } from "./actions";
export type UserOption = { id: string; username: string; name: string };
export const userOptionLabel = (user: UserOption) =>
  user.username
    ? user.name && user.name !== user.username
      ? `${user.username} · ${user.name}`
      : user.username
    : user.name;
export function useUserLookup() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const resource = useResource<{ items: UserOption[] }>(
    open ? `/users/options${query({ search: search.trim() })}` : null,
    search.trim() ? 250 : 0,
  );
  useErrorToast(resource.error);
  return { ...resource, open, setOpen, search, setSearch };
}
