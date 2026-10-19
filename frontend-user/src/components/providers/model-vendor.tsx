import { Box } from "lucide-react";
import { OpenAIIcon } from "./chatgpt/brand-icon";
import { ClaudeIcon } from "./claude/brand-icon";
import { GeminiIcon } from "./gemini/brand-icon";
import { XAIIcon } from "./grok/brand-icon";

/** Vendor name and icon for a catalog model's platform label. */
export function modelVendor(platform: string) {
  switch (platform) {
    case "chatgpt":
      return { name: "OpenAI", Icon: OpenAIIcon };
    case "grok":
      return { name: "xAI", Icon: XAIIcon };
    case "gemini":
      return { name: "Google", Icon: GeminiIcon };
    case "claude":
      return { name: "Anthropic", Icon: ClaudeIcon };
    default:
      return { name: platform, Icon: Box };
  }
}
