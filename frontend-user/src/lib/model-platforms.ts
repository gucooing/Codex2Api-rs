/** Model platforms: the vendor label of a catalog model, independent of the supplier
 * channels that serve it (codex2api_common::model_platforms). */
export const platformOptions: { value: string; label: string }[] = [
  { value: "openai", label: "OpenAI" },
  { value: "anthropic", label: "Anthropic" },
  { value: "google", label: "Google" },
  { value: "xai", label: "xAI" },
  { value: "deepseek", label: "DeepSeek" },
  { value: "alibaba", label: "通义千问" },
  { value: "zhipu", label: "智谱" },
  { value: "moonshot", label: "Kimi" },
  { value: "minimax", label: "MiniMax" },
  { value: "bytedance", label: "豆包" },
  { value: "xiaomi", label: "小米 MiMo" },
  { value: "tencent", label: "腾讯混元" },
  { value: "baidu", label: "百度文心" },
  { value: "stepfun", label: "阶跃星辰" },
  { value: "meta", label: "Meta" },
  { value: "mistral", label: "Mistral" },
  { value: "other", label: "其他" },
];
export const platformLabel = (platform: string) =>
  platformOptions.find((option) => option.value === platform)?.label ?? platform;
