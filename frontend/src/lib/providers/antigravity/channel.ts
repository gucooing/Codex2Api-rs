export const antigravityChannel = {
  configGroups: [],
  oauthPrefix: "/suppliers/antigravity/oauth",
  methods: ["callback", "refresh_token"],
  profileRefresh: (id: string) => `/suppliers/antigravity/${encodeURIComponent(id)}/profile`,
  officialSections: [["details", "官方资料"]],
  callbackLabel: "代码／回调授权",
  callbackDescription: "打开 Google 授权页面，粘贴返回的授权代码或 localhost 回调链接",
  callbackInputLabel: "授权代码或完整回调链接",
  callbackBody: (state: string, input: string) => ({ state, code: input }),
};
