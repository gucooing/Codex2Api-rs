import { apiChannel as apiConfig } from "@/lib/providers/api/channel";
import { ApiFingerprintFields } from "./fingerprint";
import { ApiOfficialData } from "./official-data";
export const apiChannel = {
  ...apiConfig,
  FingerprintFields: ApiFingerprintFields,
  OfficialData: ApiOfficialData,
};
