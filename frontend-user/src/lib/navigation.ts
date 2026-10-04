import {
  ReceiptText,
  BarChart3,
  CreditCard,
  LayoutDashboard,
  Monitor,
  ShoppingBag,
  UserRound,
  Wallet,
} from "lucide-react";

export const userNavigation = [
  { href: "/", label: "总览", icon: LayoutDashboard, group: "工作台" },
  { href: "/usage/", label: "使用信息", icon: BarChart3, group: "工作台" },
  { href: "/subscriptions/", label: "我的订阅", icon: CreditCard, group: "订阅服务" },
  { href: "/plans/", label: "购买套餐", icon: ShoppingBag, group: "订阅服务" },
  { href: "/orders/", label: "我的订单", icon: ReceiptText, group: "订阅服务" },
  { href: "/wallet/", label: "钱包", icon: Wallet, group: "订阅服务" },
  { href: "/devices/", label: "登录设备", icon: Monitor, group: "账户" },
  { href: "/profile/", label: "用户信息", icon: UserRound, group: "账户" },
] as const;

export function isUserPage(pathname: string, href: string) {
  return (pathname.replace(/\/+$/, "") || "/") === (href.replace(/\/+$/, "") || "/");
}
