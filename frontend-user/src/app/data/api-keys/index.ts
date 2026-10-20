"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { request } from "@/lib/api";
import { copyTextElement } from "@/lib/clipboard";
import type { ApiGroups, ApiKey, CreatedApiKey } from "@/lib/api-access";
import { usePageControls, type ListPage } from "@/lib/pagination";
import { useResource } from "@/lib/resource";
import { useId, useRef, useState, type FormEvent } from "react";

/** A full secret shown once, after creation or rotation. */
export type ApiKeySecret = CreatedApiKey & { action: "created" | "rotated" };

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
  const [secret, setSecret] = useState<ApiKeySecret>();
  const [rotating, setRotating] = useState<ApiKey>();
  const [deleting, setDeleting] = useState<ApiKey>();
  const secretRef = useRef<HTMLParagraphElement>(null);
  const baseRef = useRef<HTMLParagraphElement>(null);
  const saving = actions.isBusy("api-key-save");
  const toggling = actions.isBusy("api-key-toggle");
  const rotatingBusy = actions.isBusy("api-key-rotate");
  const deletingBusy = actions.isBusy("api-key-delete");
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
        // A key may keep its current group even when that group is no longer offered;
        // any other choice must be an available group.
        const available =
          groups.data?.items.some((group) => group.id === groupId) ||
          (!!editing && editing.group_id === groupId);
        if (!available) throw new Error("请选择可用的 API 组");
        if (editing)
          await request(`/api-keys/${encodeURIComponent(editing.id)}`, {
            method: "POST",
            body: { name: name.trim(), group_id: groupId, enabled, revision: editing.revision },
          });
        else {
          const result = await request<CreatedApiKey>("/api-keys", {
            method: "POST",
            body: { name: name.trim(), group_id: groupId },
          });
          setSecret({ ...result, action: "created" });
        }
        setEditing(undefined);
        resource.reload();
      },
      "密钥已保存",
    );
  const toggle = (key: ApiKey) =>
    void actions.run(
      "api-key-toggle",
      async () => {
        if (!resource.ready) throw new Error("请先加载密钥");
        await request(`/api-keys/${encodeURIComponent(key.id)}`, {
          method: "POST",
          body: {
            name: key.name,
            group_id: key.group_id,
            enabled: !key.enabled,
            revision: key.revision,
          },
        });
        resource.reload();
      },
      { success: key.enabled ? "密钥已停用" : "密钥已启用" },
    );
  const rotate = () =>
    void actions.run(
      "api-key-rotate",
      async () => {
        if (!rotating || !resource.ready) throw new Error("请先加载密钥");
        const result = await request<CreatedApiKey>(
          `/api-keys/${encodeURIComponent(rotating.id)}/rotate`,
          { method: "POST", body: { revision: rotating.revision } },
        );
        setRotating(undefined);
        setSecret({ ...result, action: "rotated" });
        resource.reload();
      },
      { success: "密钥已轮换" },
    );
  const remove = () =>
    void actions.run(
      "api-key-delete",
      async () => {
        if (!deleting || !resource.ready) throw new Error("请先加载密钥");
        await request(`/api-keys/${encodeURIComponent(deleting.id)}/delete`, {
          method: "POST",
          body: { revision: deleting.revision },
        });
        setDeleting(undefined);
        resource.reload();
      },
      { success: "密钥已删除" },
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
    toggling,
    toggle,
    secret,
    setSecret,
    secretRef,
    baseRef,
    copySecret,
    copyBase,
    rotating,
    setRotating,
    rotatingBusy,
    rotate,
    deleting,
    setDeleting,
    deletingBusy,
    remove,
  };
}
