import { apiChannel as apiConfig } from "@/lib/providers/api/channel";
import { ApiFingerprintFields } from "./fingerprint";
export const apiChannel = {
  ...apiConfig,
  FingerprintFields: ApiFingerprintFields,
  OfficialData: null,
};
