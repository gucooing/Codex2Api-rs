"use client";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import { request, type List, type Model, type ModelPreset, type TokenPrice } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { modelWrite } from "@/lib/domain";
import { useResource } from "@/lib/hooks";
import {
  findModelPreset,
  pricingDraft,
  pricingRows,
  withModelPreset,
  type BasePrice,
  type PricingDraft,
} from "@/lib/model-pricing";
import { useListResource } from "@/lib/pagination";
import { useSavedFilters } from "@/lib/preferences";
import { useId, useState } from "react";

export const tokenRule = (): TokenPrice => ({
  tier: "standard",
  min_input_tokens: 0,
  input_rate: "",
  cached_rate: "",
  cache_write_rate: "",
  output_rate: "",
});

export const emptyModel = (): Model => ({
  provider_id: "chatgpt",
  model: "",
  kind: "text",
  enabled: true,
  revision: null,
  token_prices: [tokenRule()],
  image_prices: [{ resolution: "", price: "" }],
});

export function useModelEditor({
  model,
  presets,
  onSaved,
}: {
  model: Model;
  presets: ModelPreset[] | undefined;
  onSaved: () => void;
}) {
  const dialogFocus = useDialogFocus();
  const fieldId = useId();
  const actions = useActions();
  const [value, setValue] = useState(model);
  const [pricing, setPricing] = useState(() => pricingDraft(model.token_prices));
  const [pricingChanged, setPricingChanged] = useState(false);
  const [customPricing, setCustomPricing] = useState(model.revision !== null);
  const [presetVersion, setPresetVersion] = useState<string>();
  const matchedPreset = findModelPreset(presets, value.provider_id, value.model);
  const applyPreset = (current: Model, preset: ModelPreset) => {
    const next = withModelPreset(current, preset);
    setValue(next);
    setPricing(pricingDraft(next.token_prices));
    setPricingChanged(true);
    setCustomPricing(false);
    setPresetVersion(preset.version);
  };
  const update = <K extends keyof Model>(key: K, next: Model[K]) => {
    const current = { ...value, [key]: next };
    if ((key === "model" || key === "provider_id") && model.revision === null && !customPricing) {
      const preset = findModelPreset(presets, current.provider_id, current.model);
      if (preset?.token_prices.length) {
        applyPreset(current, preset);
        return;
      }
      current.token_prices = [tokenRule()];
      setPricing(pricingDraft(current.token_prices));
      setPricingChanged(false);
      setPresetVersion(undefined);
    }
    if (key === "kind" || key === "image_prices") {
      setCustomPricing(true);
      setPresetVersion(undefined);
    }
    setValue(current);
  };
  const changePricing = (next: PricingDraft) => {
    setPricing(next);
    setPricingChanged(true);
    setCustomPricing(true);
    setPresetVersion(undefined);
  };
  const updateRange = (index: number, patch: Partial<BasePrice>) =>
    changePricing({
      ...pricing,
      ranges: pricing.ranges.map((row, i) => (i === index ? { ...row, ...patch } : row)),
    });
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "app\\models\\page.tsx:form:6",
      async () => {
        if (model.revision === null && !presets) throw new Error("价格预设尚未加载，请重试。");
        await request("/models", {
          method: "POST",
          body: modelWrite(
            {
              ...value,
              token_prices:
                value.kind === "text" && pricingChanged ? pricingRows(pricing) : value.token_prices,
            },
            presetVersion,
          ),
        });
        onSaved();
      },
      "已保存",
    );
  return {
    dialogFocus,
    fieldId,
    actions,
    value,
    pricing,
    presetVersion,
    matchedPreset,
    applyPreset,
    update,
    changePricing,
    updateRange,
    handleSubmit,
  } as const;
}

export function useModelPricing() {
  const tableColumns0 = useColumnVisibility(
    "app/models/page.tsx:0",
    ["模型", "计费方式", "价格规则", "状态", "操作"],
    ["模型", "状态", "操作"],
  );
  const fieldId = useId();
  const actions = useActions();
  const presets = useResource<List<ModelPreset>>("/models/presets");
  const [editing, setEditing] = useState<Model>();
  const empty = { search: "", kind: "", status: "" };
  const { filters, setFilters, applied, setApplied } = useSavedFilters("models.filters", empty);
  const resource = useListResource<Model>("/models", applied);
  const pagination = resource.pagination;
  useErrorToast(resource.error);
  useErrorToast(presets.error);
  const handleSelect = (model: Model) =>
    void actions.run(
      "app\\models\\page.tsx:action:4",
      async () => {
        await request("/models/status", {
          method: "POST",
          body: {
            provider_id: model.provider_id,
            model: model.model,
            revision: model.revision,
            enabled: !model.enabled,
          },
        });
        resource.reload();
      },
      {
        confirm: model.enabled ? "停用此模型并停止接受新请求？" : undefined,
        danger: false,
        success: undefined,
      },
    );
  const handleSelect2 = (model: Model) =>
    void actions.run(
      "app\\models\\page.tsx:action:5",
      async () => {
        await request("/models/delete", {
          method: "POST",
          body: {
            provider_id: model.provider_id,
            model: model.model,
            revision: model.revision,
          },
        });
        resource.reload();
      },
      {
        confirm: "删除模型并停止接受新请求？历史用量和费用保留。",
        danger: true,
        success: undefined,
      },
    );
  return {
    tableColumns0,
    fieldId,
    actions,
    presets,
    editing,
    setEditing,
    empty,
    filters,
    setFilters,
    setApplied,
    resource,
    pagination,
    handleSelect,
    handleSelect2,
  } as const;
}
