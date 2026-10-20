import type { Fingerprint } from "@/lib/api";
import { antigravityChannel } from "./antigravity/channel";
import { apiChannel } from "./api/channel";
import { chatgptChannel } from "./chatgpt/channel";
import { grokChannel } from "./grok/channel";

/** Every supplier platform the gateway serves; subscription forms keep their own list. */
export const providerOptions: { value: string; label: string }[] = [
  { value: "chatgpt", label: "ChatGPT" },
  { value: "grok", label: "Grok" },
  { value: "antigravity", label: "Antigravity" },
  { value: "api", label: "API" },
];
export const providerLabel = (provider: string) =>
  providerOptions.find((option) => option.value === provider)?.label ?? provider;
/** Identification labels of catalog models: the model's vendor. */
export const platformOptions: { value: string; label: string }[] = [
  { value: "chatgpt", label: "ChatGPT" },
  { value: "grok", label: "Grok" },
  { value: "gemini", label: "Gemini" },
  { value: "claude", label: "Claude" },
  { value: "other", label: "Other" },
];
export const platformLabel = (platform: string) =>
  platformOptions.find((option) => option.value === platform)?.label ?? platform;
export function supplierChannel(provider = "chatgpt") {
  return provider === "grok"
    ? grokChannel
    : provider === "antigravity"
      ? antigravityChannel
      : provider === "api"
        ? apiChannel
        : chatgptChannel;
}
/** The fields a channel's configuration request carries: endpoint settings for API keys, the frozen identity otherwise. */
export function channelFingerprint(provider: string, value: Fingerprint): Partial<Fingerprint> {
  if (provider === "api") {
    const {
      base_url = "",
      wire = "chat_completions",
      auth_scheme = "bearer",
      retry_statuses = [],
      proxy_id = null,
    } = value;
    return { base_url, wire, auth_scheme, retry_statuses, proxy_id };
  }
  const { os_type, os_version, arch, terminal, proxy_id, timezone } = value;
  return { os_type, os_version, arch, terminal, proxy_id, timezone };
}
