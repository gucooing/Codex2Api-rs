"use client";

import { useActions, useErrorToast } from "@/lib/actions";
import { query, request, type List, type ModelOption, type SupplierTag } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import type { ApiGroup, ApiModelRule } from "@/lib/metered-api";
import { useListResource } from "@/lib/pagination";
import { useSavedFilters } from "@/lib/preferences";
import { useId, useState, type FormEvent } from "react";

export const emptyGroup = (): ApiGroup => ({
  id: "",
  name: "",
  description: "",
  provider_id: "chatgpt",
  enabled: true,
  tag_ids: [],
  allowed_models: [],
  multiplier: "1",
  search_price_usd: null,
  concurrency_limit: 0,
  rpm_limit: 0,
  model_rules: [],
  revision: null,
});

export function useApiGroupsPage() {
  const id = useId();
  const actions = useActions();
  const defaults = { search: "", provider_id: "" };
  const { filters, setFilters, applied, setApplied } = useSavedFilters(
    "api-groups.filters",
    defaults,
  );
  const resource = useListResource<ApiGroup>("/api-groups", applied);
  const [editing, setEditing] = useState<ApiGroup>();
  const columns = useColumnVisibility(
    "api-groups",
    ["名称", "平台", "模型范围", "倍率", "每用户限制", "状态", "操作"],
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

type RuleDraft = {
  model: string;
  multiplier: string;
  concurrency_limit: string;
  rpm_limit: string;
};
export function useApiGroupEditor(group: ApiGroup, ready: boolean, onSaved: () => void) {
  const id = useId();
  const actions = useActions();
  const [value, setValue] = useState(group);
  const [limits, setLimits] = useState({
    concurrency_limit: String(group.concurrency_limit),
    rpm_limit: String(group.rpm_limit),
  });
  const [rules, setRules] = useState<RuleDraft[]>(
    group.model_rules.map((rule) => ({
      ...rule,
      multiplier: rule.multiplier ?? "",
      concurrency_limit: String(rule.concurrency_limit),
      rpm_limit: String(rule.rpm_limit),
    })),
  );
  const [modelSearch, setModelSearch] = useState("");
  const [modelMode, setModelMode] = useState(group.allowed_models.length ? "selected" : "all");
  const tags = useResource<List<SupplierTag>>(
    `/supplier-tags/options${query({ provider_id: value.provider_id })}`,
  );
  const models = useResource<List<ModelOption>>(
    `/models/options${query({ provider_id: value.provider_id, search: modelSearch })}`,
    250,
  );
  const busy = actions.isBusy("api-group-save");
  useErrorToast(tags.error);
  useErrorToast(models.error);
  const update = <K extends keyof ApiGroup>(key: K, next: ApiGroup[K]) =>
    setValue((current) => ({ ...current, [key]: next }));
  const setProvider = (provider_id: string) => {
    setValue((current) => ({
      ...current,
      provider_id,
      tag_ids: [],
      allowed_models: [],
      search_price_usd: null,
    }));
    setRules([]);
    setModelMode("all");
    setModelSearch("");
  };
  const toggleTag = (tag: string, checked: boolean) =>
    update(
      "tag_ids",
      checked
        ? [...new Set([...value.tag_ids, tag])]
        : value.tag_ids.filter((item) => item !== tag),
    );
  const toggleModel = (model: string, checked: boolean) =>
    update(
      "allowed_models",
      checked
        ? [...new Set([...value.allowed_models, model])]
        : value.allowed_models.filter((item) => item !== model),
    );
  const setRule = (model: string, field: keyof Omit<RuleDraft, "model">, next: string) =>
    setRules((current) => {
      const existing = current.find((rule) => rule.model === model) ?? {
        model,
        multiplier: "",
        concurrency_limit: "0",
        rpm_limit: "0",
      };
      return [...current.filter((rule) => rule.model !== model), { ...existing, [field]: next }];
    });
  const selectVisible = (checked: boolean) => {
    const visible = models.data?.items.map((model) => model.model) ?? [];
    update(
      "allowed_models",
      checked
        ? [...new Set([...value.allowed_models, ...visible])]
        : value.allowed_models.filter((model) => !visible.includes(model)),
    );
  };
  const numericLimit = (text: string, label: string) => {
    if (!/^\d+$/.test(text) || !Number.isSafeInteger(Number(text)) || Number(text) > 1_000_000)
      throw new Error(`${label}须为 0 到 1000000 的整数`);
    return Number(text);
  };
  const multiplier = (text: string, label: string) => {
    if (!/^\d+(?:\.\d{1,6})?$/.test(text.trim()) || Number(text) > 1000)
      throw new Error(`${label}须为 0 到 1000，最多六位小数`);
    return text.trim();
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
            value.provider_id !== "chatgpt" ||
            !amount ||
            BigInt(amount[1]) * 1_000_000_000n + BigInt((amount[2] ?? "").padEnd(9, "0")) >
              9_223_372_036_854_775_807n
          )
            throw new Error("搜索单价须为有效的非负美元金额，最多九位小数");
        }
        if (modelMode === "selected" && !value.allowed_models.length)
          throw new Error("请选择可用模型或切换为该平台的全部模型");
        const model_rules: ApiModelRule[] = rules
          .map((rule) => ({
            model: rule.model,
            multiplier: rule.multiplier.trim()
              ? multiplier(rule.multiplier, `${rule.model} 倍率`)
              : null,
            concurrency_limit: numericLimit(rule.concurrency_limit, `${rule.model} 并发`),
            rpm_limit: numericLimit(rule.rpm_limit, `${rule.model} RPM`),
          }))
          .filter(
            (rule) => rule.multiplier !== null || rule.concurrency_limit > 0 || rule.rpm_limit > 0,
          );
        const { id: _id, ...input } = value;
        void _id;
        await request(group.id ? `/api-groups/${encodeURIComponent(group.id)}` : "/api-groups", {
          method: group.id ? "PUT" : "POST",
          body: {
            ...input,
            name: value.name.trim(),
            allowed_models: modelMode === "all" ? [] : value.allowed_models,
            multiplier: multiplier(value.multiplier, "组倍率"),
            search_price_usd: searchPrice,
            concurrency_limit: numericLimit(limits.concurrency_limit, "组并发"),
            rpm_limit: numericLimit(limits.rpm_limit, "组 RPM"),
            model_rules,
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
    setProvider,
    limits,
    setLimits,
    rules,
    setRule,
    modelSearch,
    setModelSearch,
    modelMode,
    setModelMode,
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
