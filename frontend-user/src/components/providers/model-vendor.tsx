import { Box } from "lucide-react";
import { OpenAIIcon } from "./chatgpt/brand-icon";
import { XAIIcon } from "./grok/brand-icon";

export function modelVendor(provider: string) {
  switch (provider) {
    case "chatgpt":
      return { name: "OpenAI", Icon: OpenAIIcon };
    case "grok":
      return { name: "xAI", Icon: XAIIcon };
    default:
      return { name: provider, Icon: Box };
  }
}
