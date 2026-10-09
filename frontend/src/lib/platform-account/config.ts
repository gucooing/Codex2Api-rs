"use client";
import { accountConfigGroups, accountSections } from "@/lib/account-fields";
import { useActions, useErrorToast } from "@/lib/actions";
import type { ModelOption } from "@/lib/api";
import { request, type BusinessField, type Config, type Json, type List } from "@/lib/api";
import { allowedFields, canEditConfig } from "@/lib/domain";
import { useResource } from "@/lib/hooks";
import { usePlatformPrefix } from "@/lib/platform-scope";
import { useId, useState } from "react";

export function useComplexService({
  config,
  id,
  value,
}: {
  config: Config;
  id: string;
  value: Json;
}) {
  const platformPrefix = usePlatformPrefix();
  const fieldId = useId();
  const plugins = useResource<List<{ id: string; name: string }>>(
    config.key === "system_hints" ? `${platformPrefix}/${id}/plugins` : null,
  );
  const connectors = useResource<List<{ id: string; name: string }>>(
    config.key === "system_hints" ? `${platformPrefix}/${id}/connectors` : null,
  );
  useErrorToast(plugins.error);
  useErrorToast(connectors.error);
  const root = obj(value);

  return { fieldId, plugins, connectors, root } as const;
}

export function useConfigForm({
  id,
  group,
  items,
  disabled,
  onSaved,
}: {
  id: string;
  group: string;
  items: Config[];
  disabled: boolean;
  onSaved: () => void;
}) {
  const platformPrefix = usePlatformPrefix();
  const fieldId = useId();
  const actions = useActions();
  const actionKey = `consumer-settings:${id}:${group}`;
  const busy = actions.isBusy(actionKey);
  const [drafts, setDrafts] = useState<Record<string, { value: Json; revision: number }>>({});
  const [committed, setCommitted] = useState<Record<string, { value: Json; revision: number }>>({});
  const configs = items.map((item) =>
    committed[item.key]?.revision > item.revision ? { ...item, ...committed[item.key] } : item,
  );
  const models = useResource<List<ModelOption>>(
    items.some((item) => item.key === "conversation_metadata")
      ? "/models/options?provider_id=chatgpt"
      : null,
  );
  useErrorToast(models.error);
  const valueOf = (config: Config) =>
    drafts[config.key] ? drafts[config.key].value : config.value;
  const change = (config: Config, value: Json) =>
    setDrafts((current) => ({
      ...current,
      [config.key]: { value, revision: current[config.key]?.revision ?? config.revision },
    }));
  const pending = configs.filter(
    (item) =>
      drafts[item.key] && JSON.stringify(drafts[item.key].value) !== JSON.stringify(item.value),
  );
  const switches = configs.flatMap((config) =>
    config.fields.filter((field) => field.kind === "boolean").map((field) => ({ config, field })),
  );
  const fields = configs.filter((item) => item.fields.some((field) => field.kind !== "boolean"));
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      actionKey,
      async () => {
        if (disabled) throw new Error("请先加载账户设置");
        let savedCount = 0;
        const failures: string[] = [];
        for (const config of pending) {
          try {
            const saved = await request<{ value: Json; revision: number }>(
              `${platformPrefix}/${id}/config/${config.key}`,
              {
                method: "PUT",
                body: drafts[config.key],
              },
            );
            setCommitted((current) => ({ ...current, [config.key]: saved }));
            savedCount++;
            setDrafts((current) => {
              const next = { ...current };
              delete next[config.key];
              return next;
            });
          } catch (error) {
            failures.push(
              `${config.label}：${error instanceof Error ? error.message : "保存失败"}`,
            );
          }
        }
        onSaved();
        if (failures.length)
          throw new Error(
            `${savedCount ? "部分修改已保存。" : ""}未保存的修改已保留。${failures.join("；")}`,
          );
      },
      "设置已保存",
    );
  return {
    fieldId,
    busy,
    drafts,
    setDrafts,
    configs,
    models,
    valueOf,
    change,
    pending,
    switches,
    fields,
    handleSubmit,
  } as const;
}

export const spec = (
  path: string,
  label: string,
  kind: BusinessField["kind"] = "string",
  optional = true,
): BusinessField => ({
  path: path.split("."),
  label,
  kind,
  choices: [],
  group: "服务设置",
  description: "",
  optional,
});

export const serviceFields: Record<string, BusinessField[]> = {
  profile: [spec("picture", "头像地址", "url"), spec("bio", "个人简介", "text")],
  age: [
    spec("is_adult", "是否成年", "boolean"),
    spec("has_verified_age_or_dob", "已记录年龄资料", "boolean", false),
  ],
  trusted_contact: [spec("enabled", "开放信任联系人功能", "boolean", false)],
  referrals: [spec("should_show", "显示邀请入口", "boolean", false)],
  gift_credits: [spec("eligible", "显示赠送入口", "boolean", false)],
  first_party: [
    spec("finances", "显示财务入口", "boolean", false),
    spec("health_eligibility.sidebar_visible", "显示健康侧栏入口", "boolean", false),
  ],
  sites: [spec("enabled", "开放站点功能", "boolean", false)],
  computer_use_policy: [
    spec("browser_enabled", "允许浏览器功能", "boolean", false),
    spec("computer_enabled", "允许电脑操控功能", "boolean", false),
  ],
  desktop_model_policy: [
    spec("reasoning_settings_enabled", "显示推理强度设置", "boolean", false),
    spec("ultra_effort_available", "提供 Ultra 推理强度", "boolean", false),
  ],
};

export function useConfigPanel({ id, group }: { id: string; group: string }) {
  const platformPrefix = usePlatformPrefix();
  const resource = useResource<List<Config>>(`${platformPrefix}/${id}/configs`);
  const category = accountConfigGroups.find((item) => item.key === group);
  const items = (category?.sections ?? []).flatMap((key) => {
    const section = accountSections.find((item) => item.key === key);
    if (!section || !canEditConfig(section.key, section.readonly)) return [];
    const stored = resource.data?.items.find((item) => item.key === key);
    return [
      {
        ...section,
        fields: allowedFields(
          key,
          section.fields.length ? section.fields : (serviceFields[key] ?? []),
        ),
        value: stored?.value ?? null,
        revision: stored?.revision ?? 0,
        updated_at_ms: stored?.updated_at_ms ?? 0,
        readonly: section.readonly || Boolean(stored?.readonly),
      },
    ];
  });
  useErrorToast(resource.error);

  return { resource, category, items } as const;
}

export function useRegionalPricing() {
  const fieldId = useId();
  const [country, setCountry] = useState("");

  return { fieldId, country, setCountry } as const;
}

export const obj = (value: Json | undefined): { [key: string]: Json } =>
  value && typeof value === "object" && !Array.isArray(value) ? value : {};
