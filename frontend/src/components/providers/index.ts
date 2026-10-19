import { antigravityChannel } from "./antigravity/channel";
import { chatgptChannel } from "./chatgpt/channel";
import { grokChannel } from "./grok/channel";
export function supplierChannel(provider = "chatgpt") {
  return provider === "grok"
    ? grokChannel
    : provider === "antigravity"
      ? antigravityChannel
      : chatgptChannel;
}
