"use client";
import { toastError, useActions, useErrorToast } from "@/lib/actions";
import { query, request, type Fingerprint, type List, type OAuth } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { channelFingerprint } from "@/lib/providers";
import { apiNameMax, apiRetryCountMax } from "@/lib/providers/api/channel";
import { useId, useState, type KeyboardEvent } from "react";
import { toast } from "sonner";

type CatalogModel = { platform: string; model: string; kind: string; enabled: boolean };

function apiKeyProblem(value: string) {
  return !value.trim() || /\s/.test(value.trim())
    ? "API Key 须为不含空白字符的单行文本"
    : undefined;
}

export function useApiKeyEditor({
  id,
  disabled,
  onSaved,
}: {
  id: string;
  disabled: boolean;
  onSaved: () => void;
}) {
  const fieldId = useId();
  const actions = useActions();
  const resource = useResource<{ api_key: string }>(
    `/suppliers/api/accounts/${encodeURIComponent(id)}/key`,
  );
  const [draft, setDraft] = useState<string>();
  const value = draft ?? resource.data?.api_key ?? "";
  const busy = actions.isBusy("supplier-api-key");
  const ready = !disabled && resource.ready;
  const changed = draft !== undefined && draft.trim() !== resource.data?.api_key;
  useErrorToast(resource.error);
  const submit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "supplier-api-key",
      async () => {
        if (!ready || !changed) throw new Error("请加载账户并修改 API Key 后再保存");
        const problem = apiKeyProblem(value);
        if (problem) throw new Error(problem);
        await request(`/suppliers/${encodeURIComponent(id)}/relogin`, {
          method: "POST",
          body: { api_key: value.trim() },
        });
        setDraft(undefined);
        resource.reload();
        onSaved();
      },
      "API Key 已保存",
    );
  return { fieldId, resource, value, setDraft, ready, changed, busy, submit };
}

export function useApiRetryStatuses(value: Fingerprint, onChange: (value: Fingerprint) => void) {
  const [input, setInput] = useState("");
  const statuses = value.retry_statuses ?? [];
  const change = (retry_statuses: number[]) => onChange({ ...value, retry_statuses });
  const keyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
    event.preventDefault();
    const text = input.trim();
    if (!text) return;
    const status = Number(text);
    if (!/^\d{3}$/.test(text) || status < 400 || status > 599) {
      toastError("同号重试状态码须为 400 到 599 之间的 HTTP 状态");
      return;
    }
    if (!statuses.includes(status)) change([...statuses, status]);
    setInput("");
  };
  return { input, setInput, statuses, change, keyDown };
}

/** An API-key account is created from its configuration and key, then its model list,
 * then a confirmation; an existing account only replaces its key. */
export function useApiAccountWizard({
  active,
  supplierId,
  config,
  actions,
  onComplete,
}: {
  active: boolean;
  supplierId?: string;
  config: Fingerprint | undefined;
  actions: ReturnType<typeof useActions>;
  onComplete: () => void;
}) {
  const [apiKey, setApiKey] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [upstream, setUpstream] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const term = search.trim();
  const catalog = useResource<List<CatalogModel>>(
    active && !supplierId ? `/models/options${query({ search: term })}` : null,
    term ? 250 : 0,
  );
  useErrorToast(catalog.error);
  const matches = (model: string) => model.toLowerCase().includes(term.toLowerCase());
  const choices = [
    ...new Set([
      ...upstream.filter(matches),
      ...(catalog.data?.items.map((item) => item.model) ?? []),
      ...models.filter(matches),
      ...(term ? [term] : []),
    ]),
  ];
  const fetching = actions.isBusy("supplier-api-models");

  const keyProblem = () => apiKeyProblem(apiKey);
  /** The first step's rules beyond the form's own required and range checks. */
  const configProblem = () => {
    const name = config?.name?.trim() ?? "";
    if (!name || [...name].length > apiNameMax) return `名称须为 1 到 ${apiNameMax} 个字符`;
    let url: URL | undefined;
    try {
      url = new URL(config?.base_url?.trim() ?? "");
    } catch {
      url = undefined;
    }
    if (!url || !["http:", "https:"].includes(url.protocol)) return "Base URL 须为 http(s) 地址";
    const statuses = config?.retry_statuses ?? [];
    if (statuses.some((status) => status < 400 || status > 599))
      return "同号重试状态码须为 400 到 599 之间的 HTTP 状态";
    const count = config?.retry_count;
    if (count === undefined || !Number.isInteger(count) || count < 1 || count > apiRetryCountMax)
      return `同号重试次数须为 1 到 ${apiRetryCountMax} 的整数`;
    return keyProblem();
  };
  const modelProblem = () => (models.length ? undefined : "请至少配置一个模型");
  const body = () => ({
    fingerprint: config && channelFingerprint("api", config),
    api_key: apiKey.trim(),
  });

  const fetchUpstream = () =>
    void actions.run(
      "supplier-api-models",
      async () => {
        const problem = configProblem();
        if (problem) throw new Error(problem);
        const result = await request<List<string>>("/suppliers/api/accounts/models", {
          method: "POST",
          body: body(),
        });
        setUpstream(result.items);
        setModels((current) => [...new Set([...current, ...result.items])]);
        toast.success(
          result.items.length ? `已添加 ${result.items.length} 个上游模型` : "上游没有列出模型",
        );
      },
      { success: "" },
    );
  const create = async () => {
    const problem = configProblem() ?? modelProblem();
    if (problem) throw new Error(problem);
    const result = await request<OAuth>("/suppliers/api/accounts/start", {
      method: "POST",
      body: { ...body(), models },
    });
    if (result.status !== "complete") throw new Error("供应账户未创建");
    toast.success(result.reused_existing ? "已更新使用同一 Key 的原账户" : "API 供应账户已创建");
    setApiKey("");
    onComplete();
  };
  const replaceKey = async () => {
    const problem = keyProblem();
    if (!supplierId || problem) throw new Error(problem ?? "请先加载供应账户");
    await request<OAuth>(`/suppliers/${encodeURIComponent(supplierId)}/relogin`, {
      method: "POST",
      body: { api_key: apiKey.trim() },
    });
    toast.success("API Key 已更换");
    setApiKey("");
    onComplete();
  };
  /** Check a step before moving on; a problem is reported as a toast. */
  const accept = (step: number) => {
    const problem = step === 0 ? configProblem() : step === 1 ? modelProblem() : undefined;
    if (problem) toastError(problem);
    return !problem;
  };
  const reset = () => {
    setApiKey("");
    setModels([]);
    setUpstream([]);
    setSearch("");
  };
  return {
    apiKey,
    setApiKey,
    models,
    setModels,
    remove: (model: string) => setModels((current) => current.filter((item) => item !== model)),
    search,
    setSearch,
    choices,
    catalog,
    fetching,
    fetchUpstream,
    accept,
    create,
    replaceKey,
    reset,
  } as const;
}
