import { chatgptChannel } from "./chatgpt/channel";
import { grokChannel } from "./grok/channel";
export function supplierChannel(provider = "chatgpt") {
  return provider === "grok" ? grokChannel : chatgptChannel;
}
