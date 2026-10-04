import * as chatgpt from "./providers/chatgpt/subscriptions";
import * as grok from "./providers/grok/subscriptions";
export const subscriptionChoices = chatgpt.subscriptionChoices;
export const grokSubscriptionChoices = grok.subscriptionChoices;
export function subscriptionValue(value: string, provider = "chatgpt"): string {
  return provider === "grok" ? grok.subscriptionValue(value) : value;
}
export function subscriptionLabel(value: unknown, provider = "chatgpt"): string {
  return (provider === "grok" ? grok : chatgpt).subscriptionLabel(value);
}

export function supplierSubscriptionLabel(value: unknown, provider = "chatgpt"): string {
  return provider === "grok"
    ? grok.supplierSubscriptionLabel(value)
    : chatgpt.subscriptionLabel(value);
}
