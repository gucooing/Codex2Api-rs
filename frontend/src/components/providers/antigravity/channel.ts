import { antigravityChannel as antigravityConfig } from "@/lib/providers/antigravity/channel";
import { AntigravityFingerprintFields } from "./fingerprint";
import { AntigravityOfficialData } from "./official-data";
export const antigravityChannel = {
  ...antigravityConfig,
  FingerprintFields: AntigravityFingerprintFields,
  OfficialData: AntigravityOfficialData,
};
