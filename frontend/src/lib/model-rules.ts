import type { ModelOption, ModelRule } from "./api";

/** Editable text form of a granted model rule shared by plans and API groups. */
export type ModelRuleDraft = {
  model: string;
  multiplier: string;
  concurrency_limit: string;
  rpm_limit: string;
};

export type ModelRuleRow = { model: string; platform: string | null; enabled: boolean };

/** The table and bulk actions must use the same filtered set, including removed catalog entries. */
export function modelRuleRows(
  options: ModelOption[] | undefined,
  rules: ModelRuleDraft[],
  search: string,
  platform: string,
): ModelRuleRow[] {
  const visible = options ?? [];
  const term = search.trim().toLowerCase();
  return [
    ...visible.map(({ model, platform, enabled }) => ({ model, platform, enabled })),
    ...rules
      .filter(
        (rule) =>
          !platform &&
          rule.model.toLowerCase().includes(term) &&
          !visible.some((option) => option.model === rule.model),
      )
      .map((rule) => ({ model: rule.model, platform: null, enabled: true })),
  ];
}

export function ruleDraft(rule: ModelRule): ModelRuleDraft {
  return {
    model: rule.model,
    multiplier: rule.multiplier ?? "",
    concurrency_limit: String(rule.concurrency_limit),
    rpm_limit: String(rule.rpm_limit),
  };
}

export function grantedDraft(model: string): ModelRuleDraft {
  return { model, multiplier: "", concurrency_limit: "0", rpm_limit: "0" };
}

export function toggleRule(rules: ModelRuleDraft[], model: string, checked: boolean) {
  if (!checked) return rules.filter((rule) => rule.model !== model);
  return rules.some((rule) => rule.model === model) ? rules : [...rules, grantedDraft(model)];
}

export function setRuleField(
  rules: ModelRuleDraft[],
  model: string,
  field: keyof Omit<ModelRuleDraft, "model">,
  value: string,
) {
  return rules.map((rule) => (rule.model === model ? { ...rule, [field]: value } : rule));
}

function limit(text: string, label: string) {
  if (!/^\d+$/.test(text.trim()) || Number(text) > 1_000_000)
    throw new Error(`${label}须为 0 到 1000000 的整数`);
  return Number(text);
}

export function multiplierText(text: string, label: string) {
  if (!/^\d+(?:\.\d{1,6})?$/.test(text.trim()) || Number(text) > 1000)
    throw new Error(`${label}须为 0 到 1000，最多六位小数`);
  return text.trim();
}

/** Validate drafts into the request shape; an empty multiplier inherits the base multiplier. */
export function rulesFromDrafts(rules: ModelRuleDraft[]): ModelRule[] {
  return rules.map((rule) => ({
    model: rule.model,
    multiplier: rule.multiplier.trim()
      ? multiplierText(rule.multiplier, `${rule.model} 倍率`)
      : null,
    concurrency_limit: limit(rule.concurrency_limit, `${rule.model} 并发`),
    rpm_limit: limit(rule.rpm_limit, `${rule.model} RPM`),
  }));
}
