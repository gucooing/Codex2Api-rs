"use client";
import { accountSections } from "@/lib/account-fields";
import { useErrorToast } from "@/lib/actions";
import { type BusinessField, type Json } from "@/lib/api";
import { useColumnVisibility } from "@/lib/columns";
import { useResource } from "@/lib/hooks";
import { useListResource } from "@/lib/pagination";
import { obj } from "@/lib/platform-account/config";
import { usePlatformPrefix } from "@/lib/platform-scope";
import { scalar } from "@/lib/records";
import { useId, useState } from "react";

export const clientLabels: Record<string, string> = {
  title: "标题",
  name: "名称",
  display_name: "显示名称",
  username: "公开用户名",
  email: "邮箱",
  photo_frame_style: "头像形状",
  bio: "简介",
  role: "角色",
  status: "状态",
  selected: "所选语音",
  branch_format: "分支格式",
  git_diff_mode: "差异视图",
  enabled: "启用",
  is_enabled: "已启用",
  approval_mode: "操作审批",
  history_approval_mode: "历史访问审批",
  iab_history_approval_mode: "内置浏览器历史审批",
  download_approval_mode: "下载审批",
  upload_approval_mode: "上传审批",
  disable_auto_review: "关闭自动审查",
  full_cdp_access_enabled: "完整浏览器控制",
  webmcp_enabled: "网页 MCP",
  show_usage_stats_section: "用量统计",
  show_activity_graph_section: "活动图表",
  show_insights_section: "活动分析",
  show_top_plugins_section: "常用插件",
  desktop_onboarding_completed_at: "引导完成时间",
  updated_at: "更新时间",
  created_at: "创建时间",
  id: "记录标识",
  item_id: "资源标识",
  item_type: "资源类型",
  description: "说明",
  instructions: "指令",
  notification_type: "通知类型",
  category: "分类",
  channel: "渠道",
  brand: "品牌",
  last4: "卡号后四位",
  permission: "权限",
  version: "版本",
  contents: "配置正文",
  phone_number: "电话号码",
  relationship: "关系",
  expiry_month: "到期月份",
  expiry_year: "到期年份",
  seen_at: "查看时间",
  reacted_to_at: "处理时间",
  branch_name: "分支",
  repo_url: "仓库地址",
  url: "链接",
  path: "路径",
  localized_interface: "使用客户端所选语言",
  profile_enabled: "显示个人信息页",
  type: "类型",
  message: "内容",
  notification_id: "通知标识",
  clear_cache_only: "仅刷新配置",
  program_id: "活动",
  invite_url: "邀请地址",
  referral_id: "邀请标识",
  can_resend: "可重新发送",
};

export const clientValues: Record<string, string> = {
  always_ask: "每次询问",
  never_ask: "无需询问",
  disabled: "禁止访问",
  allow: "允许",
  deny: "拒绝",
  unified: "统一视图",
  split: "并排视图",
  circle: "圆形",
  scalloped_circle: "花形",
  rounded_square: "圆角方形",
  oval: "椭圆",
  scalloped_oval: "花边椭圆",
};

export function clientScalar(value: Json | undefined) {
  return typeof value === "string" ? (clientValues[value] ?? scalar(value)) : scalar(value);
}

export const clientGroups: Record<string, string> = {
  settings: "偏好设置",
  flags: "服务可用性",
  display_settings: "个人页栏目",
  items: "记录列表",
  plugins: "已安装插件",
  gizmo: "项目信息",
  conversations: "项目会话",
  payment_methods: "支付方式",
  members: "成员",
  preferences: "偏好",
  rules: "规则",
  options: "通知渠道",
  payload: "通知内容",
  config_toml: "配置文件",
  requirements_toml: "客户端要求",
  enterprise_managed: "配置条目",
};

export function useBrowserClientState({ value, path }: { value: Json; path: string }) {
  const tableColumns0 = useColumnVisibility(
    "components/config.tsx:0",
    ["站点匹配规则", "审批策略"],
    ["站点匹配规则", "审批策略"],
  );
  const root = obj(value);
  const preferences = obj(root.preferences);
  const origin = useListResource<[string, Json]>(path, {
    section: "$.rules.origin",
    entries: true,
  });
  const download = useListResource<[string, Json]>(path, {
    section: "$.rules.download",
    entries: true,
  });
  const upload = useListResource<[string, Json]>(path, {
    section: "$.rules.upload",
    entries: true,
  });
  const fullCdp = useListResource<[string, Json]>(path, {
    section: "$.rules.full_cdp",
    entries: true,
  });
  useErrorToast(origin.error);
  useErrorToast(download.error);
  useErrorToast(upload.error);
  useErrorToast(fullCdp.error);
  const pages = {
    origin: origin.pagination,
    download: download.pagination,
    upload: upload.pagination,
    full_cdp: fullCdp.pagination,
  };

  return { tableColumns0, preferences, pages } as const;
}

export function useClientStatePanel() {
  const fieldId = useId();
  const [key, setKey] = useState("cloud_preferences");
  const items = accountSections.filter(
    (section) => section.readonly || section.key === "user_settings",
  );
  const selected = items.find((item) => item.key === key) ?? items[0];

  return { fieldId, setKey, items, selected } as const;
}

export function useClientState({
  id,
  config,
}: {
  id: string;
  config: (typeof accountSections)[number];
}) {
  const platformPrefix = usePlatformPrefix();
  const resource = useResource<{
    value: Json;
    revision: number | null;
    write_origin: string | null;
    updated_at_ms: number | null;
    fields: BusinessField[];
  }>(`${platformPrefix}/${id}/client-state/${config.key}`);
  const rowsPath = `${platformPrefix}/${id}/client-state/${config.key}/rows`;
  useErrorToast(resource.error);

  return { resource, rowsPath } as const;
}

export function useNamedClientState({
  value,
  path,
  section = "$",
}: {
  value: Json;
  path: string;
  section?: string;
}) {
  const tableColumns1 = useColumnVisibility(
    "components/config.tsx:1",
    ["名称 / 内容", "标识", "状态", "详细记录"],
    ["名称 / 内容", "状态"],
  );
  const resource = useListResource<Json>(Array.isArray(value) ? path : null, { section });
  const pagination = resource.pagination;
  useErrorToast(resource.error);
  const entries =
    value && typeof value === "object" && !Array.isArray(value) ? Object.entries(value) : [];
  const fields = entries
    .filter(([key, item]) => key in clientLabels && (item === null || typeof item !== "object"))
    .map(([key, item]) => ({ label: clientLabels[key], value: clientScalar(item) }));

  return { tableColumns1, pagination, entries, fields } as const;
}
