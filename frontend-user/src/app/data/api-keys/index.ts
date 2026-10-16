"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { request } from "@/lib/api";
import { copyTextElement } from "@/lib/clipboard";
import type { ApiGroups, ApiKey, CreatedApiKey } from "@/lib/api-access";
import { usePageControls, type ListPage } from "@/lib/pagination";
import { useResource } from "@/lib/resource";
import { useId, useRef, useState, type FormEvent } from "react";

export function useApiKeysPage() {
  const id = useId();
  const actions = useActions();
  const [filters, setFilters] = useState({ search: "", group_id: "", status: "" });
  const [applied, setApplied] = useState(filters);
  const [page, setPage] = useState(1);
  const [size, setSize] = useState(20);
  const groups = useResource<ApiGroups>("/api-groups", 0);
  const params = new URLSearchParams({ ...applied, page: String(page), page_size: String(size) });
  const resource = useResource<ListPage<ApiKey>>(`/api-keys?${params}`, 0);
  const pagination = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    setPage,
    size,
    !resource.ready,
    setSize,
  );
  const [editing, setEditing] = useState<ApiKey | null>();
  const [name, setName] = useState("");
  const [groupId, setGroupId] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [created, setCreated] = useState<CreatedApiKey>();
  const [revoking, setRevoking] = useState<ApiKey>();
  const secretRef = useRef<HTMLParagraphElement>(null);
  const baseRef = useRef<HTMLParagraphElement>(null);
  const saving = actions.isBusy("api-key-save");
  const revokingBusy = actions.isBusy("api-key-revoke");
  useErrorToast(groups.error);
  useErrorToast(resource.error);
  const search = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setApplied({ ...filters });
    setPage(1);
    resource.reload();
  };
  const reset = () => {
    const empty = { search: "", group_id: "", status: "" };
    setFilters(empty);
    setApplied(empty);
    setPage(1);
    resource.reload();
  };
  const openEditor = (key?: ApiKey) => {
    setEditing(key ?? null);
    setName(key?.name ?? "");
    setGroupId(key?.group_id ?? "");
    setEnabled(key?.enabled ?? true);
  };
  const submit = (event: FormEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "api-key-save",
      async () => {
        if (!resource.ready || !groups.ready || editing === undefined)
          throw new Error("请先加载密钥和 API 组");
        if (!name.trim()) throw new Error("请填写密钥名称");
        if (editing)
          await request(`/api-keys/${encodeURIComponent(editing.id)}`, {
            method: "POST",
            body: { name: name.trim(), enabled, revision: editing.revision },
          });
        else {
          if (!groups.data?.items.some((group) => group.id === groupId))
            throw new Error("请选择可用的 API 组");
          const result = await request<CreatedApiKey>("/api-keys", {
            method: "POST",
            body: { name: name.trim(), group_id: groupId },
          });
          setCreated(result);
        }
        setEditing(undefined);
        resource.reload();
      },
      "密钥已保存",
    );
  const revoke = () =>
    void actions.run(
      "api-key-revoke",
      async () => {
        if (!revoking || !resource.ready) throw new Error("请先加载密钥");
        await request(`/api-keys/${encodeURIComponent(revoking.id)}/revoke`, {
          method: "POST",
          body: { revision: revoking.revision },
        });
        setRevoking(undefined);
        resource.reload();
      },
      { success: "密钥已撤销" },
    );
  const copySecret = () =>
    void actions.run("copy-api-secret", () => copyTextElement(secretRef.current), {
      success: "密钥已复制",
    });
  const copyBase = () =>
    void actions.run("copy-api-base", () => copyTextElement(baseRef.current), {
      success: "API 地址已复制",
    });
  const reload = () => {
    groups.reload();
    resource.reload();
  };
  return {
    id,
    actions,
    groups,
    resource,
    pagination,
    filters,
    setFilters,
    search,
    reset,
    reload,
    editing,
    setEditing,
    openEditor,
    name,
    setName,
    groupId,
    setGroupId,
    enabled,
    setEnabled,
    saving,
    submit,
    created,
    setCreated,
    secretRef,
    baseRef,
    copySecret,
    copyBase,
    revoking,
    setRevoking,
    revokingBusy,
    revoke,
  };
}
