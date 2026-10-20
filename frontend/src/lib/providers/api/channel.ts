/** Wire formats an API-key account may speak and the vendor's own Base URL of each. */
export const apiWireOptions: { value: string; label: string; baseUrl: string }[] = [
  { value: "chat_completions", label: "Chat Completions", baseUrl: "https://api.openai.com/v1" },
  { value: "openai_responses", label: "OpenAI Responses", baseUrl: "https://api.openai.com/v1" },
  { value: "messages", label: "Anthropic Messages", baseUrl: "https://api.anthropic.com/v1" },
];
export const apiWireLabel = (wire: string | undefined) =>
  apiWireOptions.find((option) => option.value === wire)?.label ?? wire ?? "—";

const authLabels: Record<string, string> = {
  bearer: "Authorization: Bearer",
  x_api_key: "x-api-key",
};
/** The header schemes an endpoint of the wire accepts, the vendor's own first: the
 * OpenAI wires take only a Bearer token; Messages takes `x-api-key` or a Bearer token. */
export const apiAuthSchemeOptions = (wire: string | undefined) =>
  (wire === "messages" ? ["x_api_key", "bearer"] : ["bearer"]).map((value) => ({
    value,
    label: authLabels[value],
  }));
export const apiAuthLabel = (scheme: string | undefined) =>
  (scheme && authLabels[scheme]) ?? scheme ?? "—";

/** Bounds the server enforces on an account's name and same-account retries. */
export const apiNameMax = 128;
export const apiRetryCountMax = 10;

export const apiChannel = {
  configGroups: [],
  oauthPrefix: "/suppliers/api/accounts",
  profileRefresh: undefined,
  officialSections: [] as [string, string][],
};
