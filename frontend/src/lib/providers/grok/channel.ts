export const grokChannel = {
  configGroups: [{ key: "account", label: "账户与订阅", sections: [] }],
  oauthPrefix: "/suppliers/grok/oauth",
  methods: ["callback", "device", "refresh_token"],
  profileRefresh: (id: string) => `/suppliers/grok/${encodeURIComponent(id)}/profile`,
  officialSections: [
    ["usage", "服务配置"],
    ["details", "官方资料"],
    ["credits", "官方余额"],
  ],
  callbackLabel: "代码／回调授权",
  callbackDescription: "打开 Grok 官方页面，粘贴返回的授权代码或回调链接",
  callbackInputLabel: "授权代码或完整回调链接",
  callbackBody: (state: string, input: string) => ({ state, code: input }),
};
