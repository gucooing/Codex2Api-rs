import { grokChannel as grokConfig } from "@/lib/providers/grok/channel";
import { GrokFingerprintFields } from "./fingerprint";
import { GrokModelCatalog } from "./models";
import { GrokOfficialData } from "./official-data";
export const grokChannel = {
  ...grokConfig,
  FingerprintFields: GrokFingerprintFields,
  OfficialData: GrokOfficialData,
  Models: GrokModelCatalog,
};
