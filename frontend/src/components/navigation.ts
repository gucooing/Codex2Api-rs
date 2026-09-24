import {
  BarChart3,
  Boxes,
  Cable,
  LayoutDashboard,
  Layers3,
  Server,
  Settings2,
  Users,
} from "lucide-react";

export const navigation = [
  {
    href: "/",
    label: "概览",
    group: "工作台",
    icon: LayoutDashboard,
    keywords: "dashboard 总览",
    description: "账户与模型概况",
  },
  {
    href: "/usage/",
    label: "用量记录",
    group: "工作台",
    icon: BarChart3,
    keywords: "usage 费用 tokens 消费",
    description: "实际请求与费用记录",
  },
  {
    href: "/suppliers/",
    label: "供应账户",
    group: "账户管理",
    icon: Server,
    keywords: "supplier 上游 授权",
    description: "上游授权与执行账户",
  },
  {
    href: "/consumers/",
    label: "虚拟账户",
    group: "账户管理",
    icon: Users,
    keywords: "consumer 订阅 登录 身份",
    description: "消费身份、订阅与执行路由",
  },
  {
    href: "/plans/",
    label: "套餐管理",
    group: "账户管理",
    icon: Layers3,
    keywords: "plan 权益 限额",
    description: "模型权限与订阅权益",
  },
  {
    href: "/models/",
    label: "模型配置",
    group: "服务配置",
    icon: Boxes,
    keywords: "model 价格 计费",
    description: "模型目录与计费价格",
  },
  {
    href: "/proxies/",
    label: "出站代理",
    group: "服务配置",
    icon: Cable,
    keywords: "proxy 网络 连接",
    description: "供应账户的网络连接",
  },
  {
    href: "/settings/",
    label: "系统设置",
    group: "服务配置",
    icon: Settings2,
    keywords: "settings 管理员 安全",
    description: "服务策略与管理员设置",
  },
] as const;

export function isCurrentPage(pathname: string, href: string) {
  return href === "/" ? pathname === "/" : pathname.startsWith(href.slice(0, -1));
}
