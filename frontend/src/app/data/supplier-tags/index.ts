"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type SupplierTag } from "@/lib/api";
import { useListResource } from "@/lib/pagination";
import { useSavedFilters } from "@/lib/preferences";
import type { Dispatch, SetStateAction } from "react";
import { useId, useState } from "react";

export type SupplierTagEditorDialogProps = {
  editing: SupplierTag | null | undefined;
  saving: boolean;
  setEditing: Dispatch<SetStateAction<SupplierTag | null | undefined>>;
  actions: ReturnType<typeof useActions>;
  tags: ReturnType<typeof useListResource<SupplierTag>>;
  name: string;
  provider: string;
  id: string;
  setName: Dispatch<SetStateAction<string>>;
  setProvider: Dispatch<SetStateAction<string>>;
};

export function useSupplierTagEditorDialog({
  editing,
  setEditing,
  actions,
  tags,
  name,
  provider,
}: Pick<
  SupplierTagEditorDialogProps,
  "editing" | "setEditing" | "actions" | "tags" | "name" | "provider"
>) {
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "supplier-tag-save",
      async () => {
        if (editing === undefined || !tags.ready) throw new Error("请先加载标签资料");
        if (!name.trim()) throw new Error("请填写标签名称");
        await request(editing ? `/supplier-tags/${editing.id}` : "/supplier-tags", {
          method: editing ? "PUT" : "POST",
          body: { name: name.trim(), provider_id: provider },
        });
        setEditing(undefined);
        tags.reload();
      },
      "标签已保存",
    );
  return { handleSubmit } as const;
}

export function useSupplierTagsPage() {
  const actions = useActions();
  const id = useId();
  const empty = { search: "", provider_id: "" };
  const { filters, setFilters, applied, setApplied } = useSavedFilters(
    "supplier-tags.filters",
    empty,
  );
  const tags = useListResource<SupplierTag>("/supplier-tags", applied);
  const pagination = tags.pagination;
  const [editing, setEditing] = useState<SupplierTag | null>();
  const [name, setName] = useState("");
  const [provider, setProvider] = useState("chatgpt");
  const saving = actions.isBusy("supplier-tag-save");
  const openEditor = (tag?: SupplierTag) => {
    setName(tag?.name ?? "");
    setProvider(tag?.provider_id ?? (applied.provider_id || "chatgpt"));
    setEditing(tag ?? null);
  };
  useErrorToast(tags.error);
  const handleClick = (tag: SupplierTag) =>
    void actions.run(
      `tag-${tag.id}`,
      async () => {
        await request(`/supplier-tags/${tag.id}`, { method: "DELETE" });
        tags.reload();
      },
      { confirm: `删除标签“${tag.name}”？供应账户会保留。` },
    );
  return {
    actions,
    id,
    empty,
    filters,
    setFilters,
    setApplied,
    tags,
    pagination,
    editing,
    setEditing,
    name,
    setName,
    provider,
    setProvider,
    saving,
    openEditor,
    handleClick,
  } as const;
}
