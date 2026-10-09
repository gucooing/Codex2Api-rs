import { chatgptChannel as chatgptConfig } from "@/lib/providers/chatgpt/channel";
import { ChatgptFingerprintFields } from "./fingerprint";
import { ChatgptOfficialData } from "./official-data";

export const chatgptChannel = {
  ...chatgptConfig,
  FingerprintFields: ChatgptFingerprintFields,
  OfficialData: ChatgptOfficialData,
  Models: undefined,
};
