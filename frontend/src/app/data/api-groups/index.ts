"use client";

import { useActions, useErrorToast } from "@/lib/actions";
import { query, request, type List, type ModelOption, type SupplierTag } from "@/lib/api";
import type { ApiGroup } from "@/lib/api-access";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import {
  multiplierText,
  ruleDraft,
  rulesFromDrafts,
  setRuleField,
  toggleRule,
  type ModelRuleDraft,
} from "@/lib/model-rules";
import { useListResource } from "@/lib/pagination";
import { useSavedFilters } from "@/lib/preferences";
import { useId, useState, type FormEvent } from "react";

export const emptyGroup = (): ApiGroup => ({
  id: "",
  name: "",
  description: "",
  enabled: true,
  tag_ids: [],
  models: [],
  multiplier: "1",
  search_price_usd: null,
  concurrency_limit: 0,
  rpm_limit: 0,
  revision: null,
});

export function useApiGroupsPage() {
  const id = useId();
  const actions = useActions();
  const defaults = { search: "" };
  const { filters, setFilters, applied, setApplied } = useSavedFilters(
    "api-groups.filters",
    defaults,
  );
  const resource = useListResource<ApiGroup>("/api-groups", applied);
  const [editing, setEditing] = useState<ApiGroup>();
  const columns = useColumnVisibility(
    "api-groups",
    ["名称", "模型与标签", "倍率", "每用户限制", "状态", "操作"],
    ["名称", "倍率", "状态", "操作"],
  );
  useErrorToast(resource.error);
  const search = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setApplied({ ...filters });
    resource.reload(1);
  };
  const reset = () => {
    setFilters(defaults);
    setApplied(defaults);
    resource.reload(1);
  };
  const remove = (group: ApiGroup) =>
    void actions.run(
      `api-group-delete:${group.id}`,
      async () => {
        if (!resource.ready) throw new Error("请先加载 API 组");
        await request(`/api-groups/${encodeURIComponent(group.id)}`, {
          method: "DELETE",
          body: { revision: group.revision },
        });
        resource.reload();
      },
      { confirm: `删除 API 组“${group.name}”？`, danger: true, success: "API 组已删除" },
    );
  return {
    id,
    actions,
    filters,
    setFilters,
    resource,
    pagination: resource.pagination,
    columns,
    editing,
    setEditing,
    search,
    reset,
    remove,
  };
}

export function useApiGroupEditor(group: ApiGroup, ready: boolean, onSaved: () => void) {
  const id = useId();
  const actions = useActions();
  const [value, setValue] = useState(group);
  const [limits, setLimits] = useState({
    concurrency_limit: String(group.concurrency_limit),
    rpm_limit: String(group.rpm_limit),
  });
  const [rules, setRules] = useState<ModelRuleDraft[]>(group.models.map(ruleDraft));
  const [modelSearch, setModelSearch] = useState("");
  const [modelPlatform, setModelPlatform] = useState("");
  const tags = useResource<List<SupplierTag>>("/supplier-tags/options");
  const models = useResource<List<ModelOption>>(
    `/models/options${query({ search: modelSearch, platform: modelPlatform })}`,
    250,
  );
  const busy = actions.isBusy("api-group-save");
  useErrorToast(tags.error);
  useErrorToast(models.error);
  const update = <K extends keyof ApiGroup>(key: K, next: ApiGroup[K]) =>
    setValue((current) => ({ ...current, [key]: next }));
  const toggleTag = (tag: string, checked: boolean) =>
    update(
      "tag_ids",
      checked
        ? [...new Set([...value.tag_ids, tag])]
        : value.tag_ids.filter((item) => item !== tag),
    );
  const toggleModel = (model: string, checked: boolean) =>
    setRules((current) => toggleRule(current, model, checked));
  const setRule = (model: string, field: keyof Omit<ModelRuleDraft, "model">, next: string) =>
    setRules((current) => setRuleField(current, model, field, next));
  const selectVisible = (checked: boolean) =>
    setRules((current) =>
      (models.data?.items ?? []).reduce(
        (rules, option) => toggleRule(rules, option.model, checked),
        current,
      ),
    );
  const numericLimit = (text: string, label: string) => {
    if (!/^\d+$/.test(text) || !Number.isSafeInteger(Number(text)) || Number(text) > 1_000_000)
      throw new Error(`${label}须为 0 到 1000000 的整数`);
    return Number(text);
  };
  const submit = (event: FormEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "api-group-save",
      async () => {
        if (!ready || !models.ready || !tags.ready) throw new Error("请先加载 API 组资料与候选项");
        if (!value.name.trim()) throw new Error("请填写 API 组名称");
        const searchPrice = value.search_price_usd?.trim() || null;
        if (searchPrice !== null) {
          const amount = /^(\d+)(?:\.(\d{1,9}))?$/.exec(searchPrice);
          if (
            !amount ||
            BigInt(amount[1]) * 1_000_000_000n + BigInt((amount[2] ?? "").padEnd(9, "0")) >
              9_223_372_036_854_775_807n
          )
            throw new Error("搜索单价须为有效的非负美元金额，最多九位小数");
        }
        const { id: _id, ...input } = value;
        void _id;
        await request(group.id ? `/api-groups/${encodeURIComponent(group.id)}` : "/api-groups", {
          method: group.id ? "PUT" : "POST",
          body: {
            ...input,
            name: value.name.trim(),
            models: rulesFromDrafts(rules),
            multiplier: multiplierText(value.multiplier, "组倍率"),
            search_price_usd: searchPrice,
            concurrency_limit: numericLimit(limits.concurrency_limit, "组并发"),
            rpm_limit: numericLimit(limits.rpm_limit, "组 RPM"),
          },
        });
        onSaved();
      },
      "API 组已保存",
    );
  return {
    id,
    value,
    update,
    limits,
    setLimits,
    rules,
    setRule,
    modelSearch,
    setModelSearch,
    modelPlatform,
    setModelPlatform,
    tags,
    models,
    busy,
    disabled: !ready || busy || !tags.ready || !models.ready,
    toggleTag,
    toggleModel,
    selectVisible,
    submit,
  };
}
