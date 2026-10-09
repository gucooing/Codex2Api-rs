import { accountConfigGroups } from "@/lib/providers/chatgpt/account-fields";
export const chatgptChannel = {
  configGroups: accountConfigGroups,
  oauthPrefix: "/suppliers/chatgpt/oauth",
  rtRelogin: true,
  profileRefresh: undefined,
  settingsLabel: "官方用量",
  creditsLabel: "重置额度",
  callbackLabel: "回调链接",
  callbackDescription: "打开授权页面后提交完整回调链接",
  callbackInputLabel: "完整回调链接",
  callbackBody: (state: string, input: string) => ({ state, callback_url: input }),
};
