import { Box } from "lucide-react";
import {
  OpenAIIcon,
  ClaudeIcon,
  GeminiIcon,
  XAIIcon,
  DeepSeekIcon,
  QwenIcon,
  ZhipuIcon,
  KimiIcon,
  MiniMaxIcon,
  DoubaoIcon,
  XiaomiMiMoIcon,
  HunyuanIcon,
  WenxinIcon,
  StepFunIcon,
  MetaIcon,
  MistralIcon,
} from "./icons";

/** The vendor mark of a model platform; "other" and unlisted labels show a neutral box. */
export function ModelPlatformIcon({
  platform,
  className,
}: {
  platform: string;
  className?: string;
}) {
  switch (platform) {
    case "openai":
      return <OpenAIIcon className={className} />;
    case "anthropic":
      return <ClaudeIcon className={className} />;
    case "google":
      return <GeminiIcon className={className} />;
    case "xai":
      return <XAIIcon className={className} />;
    case "deepseek":
      return <DeepSeekIcon className={className} />;
    case "alibaba":
      return <QwenIcon className={className} />;
    case "zhipu":
      return <ZhipuIcon className={className} />;
    case "moonshot":
      return <KimiIcon className={className} />;
    case "minimax":
      return <MiniMaxIcon className={className} />;
    case "bytedance":
      return <DoubaoIcon className={className} />;
    case "xiaomi":
      return <XiaomiMiMoIcon className={className} />;
    case "tencent":
      return <HunyuanIcon className={className} />;
    case "baidu":
      return <WenxinIcon className={className} />;
    case "stepfun":
      return <StepFunIcon className={className} />;
    case "meta":
      return <MetaIcon className={className} />;
    case "mistral":
      return <MistralIcon className={className} />;
    default:
      return <Box className={className} />;
  }
}
