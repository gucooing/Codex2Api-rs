import { antigravityChannel } from "./antigravity/channel";
import { chatgptChannel } from "./chatgpt/channel";
import { grokChannel } from "./grok/channel";

/** Every supplier platform the gateway serves; subscription forms keep their own list. */
export const providerOptions: { value: string; label: string }[] = [
  { value: "chatgpt", label: "ChatGPT" },
  { value: "grok", label: "Grok" },
  { value: "antigravity", label: "Antigravity" },
];
export const providerLabel = (provider: string) =>
  providerOptions.find((option) => option.value === provider)?.label ?? provider;
/** Identification labels of catalog models: the model's vendor. */
export const platformOptions: { value: string; label: string }[] = [
  { value: "chatgpt", label: "ChatGPT" },
  { value: "grok", label: "Grok" },
  { value: "gemini", label: "Gemini" },
  { value: "claude", label: "Claude" },
];
export const platformLabel = (platform: string) =>
  platformOptions.find((option) => option.value === platform)?.label ?? platform;
export function supplierChannel(provider = "chatgpt") {
  return provider === "grok"
    ? grokChannel
    : provider === "antigravity"
      ? antigravityChannel
      : chatgptChannel;
}
