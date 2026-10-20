/** Wire formats an API-key account may speak and the vendor defaults of each. */
export const apiWireOptions: { value: string; label: string; baseUrl: string; auth: string }[] = [
  {
    value: "chat_completions",
    label: "Chat Completions",
    baseUrl: "https://api.openai.com/v1",
    auth: "bearer",
  },
  {
    value: "openai_responses",
    label: "OpenAI Responses",
    baseUrl: "https://api.openai.com/v1",
    auth: "bearer",
  },
  {
    value: "messages",
    label: "Anthropic Messages",
    baseUrl: "https://api.anthropic.com/v1",
    auth: "x_api_key",
  },
];
export const apiAuthSchemeOptions: { value: string; label: string }[] = [
  { value: "bearer", label: "Authorization: Bearer" },
  { value: "x_api_key", label: "x-api-key" },
];
export const apiWireLabel = (wire: string | undefined) =>
  apiWireOptions.find((option) => option.value === wire)?.label ?? wire ?? "—";

export const apiChannel = {
  configGroups: [],
  oauthPrefix: "/suppliers/api/accounts",
  methods: ["api_key"],
  profileRefresh: undefined,
  officialSections: [] as [string, string][],
  setupLabel: "接入配置",
  tokenLabel: "API Key",
  callbackLabel: "API Key 录入",
  callbackDescription: "每行一个 Key，批量创建账户",
  callbackInputLabel: "API Key",
  callbackBody: (state: string, input: string) => ({ state, code: input }),
};
