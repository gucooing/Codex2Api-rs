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
export function supplierChannel(provider = "chatgpt") {
  return provider === "grok"
    ? grokChannel
    : provider === "antigravity"
      ? antigravityChannel
      : provider === "api"
        ? apiChannel
        : chatgptChannel;
}
/** The login channels: their accounts are authorized through an official sign-in. */
export function loginChannel(provider = "chatgpt") {
  return provider === "grok"
    ? grokChannel
    : provider === "antigravity"
      ? antigravityChannel
      : chatgptChannel;
}
/** The fields a channel's configuration request carries: the API-key account's name and endpoint settings, the frozen identity otherwise. */
export function channelFingerprint(provider: string, value: Fingerprint): Partial<Fingerprint> {
  if (provider === "api") {
    const { name, base_url, wire, auth_scheme, retry_statuses, retry_count, proxy_id } = value;
    return { name, base_url, wire, auth_scheme, retry_statuses, retry_count, proxy_id };
  }
  const { os_type, os_version, arch, terminal, proxy_id, timezone } = value;
  return { os_type, os_version, arch, terminal, proxy_id, timezone };
}
