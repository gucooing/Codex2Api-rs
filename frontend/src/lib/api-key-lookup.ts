"use client";
import { useState } from "react";
import { useErrorToast } from "./actions";
import { query } from "./api";
import type { ApiKey } from "./api-access";
import { useResource } from "./hooks";

export type ApiKeyOption = Pick<ApiKey, "id" | "name" | "key_prefix" | "username">;
export const apiKeyOptionLabel = (key: ApiKeyOption) =>
  key.key_prefix ? `${key.name} · ${key.key_prefix}` : key.name;

export function useApiKeyLookup() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const resource = useResource<{ items: ApiKeyOption[] }>(
    open ? `/api-keys/options${query({ search: search.trim(), limit: 5 })}` : null,
    search.trim() ? 250 : 0,
  );
  useErrorToast(resource.error);
  return { ...resource, open, setOpen, search, setSearch };
}
