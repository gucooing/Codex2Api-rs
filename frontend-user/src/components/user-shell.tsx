"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useState,
  type ReactNode,
} from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTheme } from "next-themes";
import { LogOut, Monitor, Moon, Sun, UserRound } from "lucide-react";
import { ApiError, request, setSessionCsrf, type Session } from "@/lib/api";
import { useActions, useErrorToast } from "@/lib/actions";
import { isUserPage, userNavigation } from "@/lib/navigation";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar";

const SessionContext = createContext<{ session: Session; signOut: () => void } | undefined>(
  undefined,
);

export function useUserSession() {
  const context = useContext(SessionContext);
  if (!context) throw new Error("用户页面必须位于用户会话布局中");
  return context;
}

export function UserShell({ children }: { children: ReactNode }) {
  // undefined: restoring the cookie session; null: confirmed signed out.
  const [session, setSession] = useState<Session | null | undefined>(undefined);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const actions = useActions();
  const id = useId();
  useErrorToast(error);
  const signOut = useCallback(() => {
    setSession(null);
    setSessionCsrf("");
  }, []);
  useEffect(() => {
    window.addEventListener("user-session-expired", signOut);
    return () => window.removeEventListener("user-session-expired", signOut);
  }, [signOut]);
  useEffect(() => {
    const controller = new AbortController();
    request<Session>("/session", { signal: controller.signal })
      .then((value) => {
        if (!controller.signal.aborted) {
          setSession(value);
          setSessionCsrf(value.csrf_token);
          setError("");
        }
      })
      .catch((reason) => {
        if (controller.signal.aborted) return;
        if (reason instanceof ApiError && reason.status === 401) signOut();
        else setError(reason instanceof Error ? reason.message : "无法读取会话");
      });
    return () => controller.abort();
  }, [retry, signOut]);
  if (session === undefined)
    return (
      <main className="flex min-h-svh items-center justify-center p-4" aria-busy={!error}>
        {error ? (
          <Button
            variant="outline"
            onClick={() => {
              setError("");
              setRetry((value) => value + 1);
            }}
          >
            重新连接
          </Button>
        ) : (
          <Spinner aria-label="正在恢复登录状态" />
        )}
      </main>
    );
  if (session === null)
    return (
      <main className="flex min-h-svh items-center justify-center p-4">
        <Card className="w-full max-w-sm">
          <CardHeader>
            <CardTitle role="heading" aria-level={1}>
              用户登录
            </CardTitle>
            <CardDescription>使用管理员创建的用户账户登录。</CardDescription>
          </CardHeader>
          <CardContent>
            <form
              noValidate
              onSubmit={(event) =>
                actions.submit(
                  event,
                  "user-login",
                  async () => {
                    const value = await request<Session>("/login", {
                      method: "POST",
                      body: { username, password },
                    });
                    setSession(value);
                    setSessionCsrf(value.csrf_token);
                    setPassword("");
                    setError("");
                  },
                  "登录成功",
                )
              }
            >
              <FieldSet disabled={actions.isBusy("user-login")}>
                <FieldGroup>
                  <Field>
                    <FieldLabel htmlFor={`${id}-username`}>用户名</FieldLabel>
                    <Input
                      id={`${id}-username`}
                      autoComplete="username"
                      required
                      value={username}
                      onChange={(event) => setUsername(event.target.value)}
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-password`}>密码</FieldLabel>
                    <Input
                      id={`${id}-password`}
                      type="password"
                      autoComplete="current-password"
                      required
                      value={password}
                      onChange={(event) => setPassword(event.target.value)}
                    />
                  </Field>
                  <Button type="submit">{actions.isBusy("user-login") && <Spinner />}登录</Button>
                </FieldGroup>
              </FieldSet>
            </form>
          </CardContent>
        </Card>
      </main>
    );
  return (
    <SessionContext value={{ session, signOut }}>
      <SidebarProvider key={session.user.id}>
        <UserNavigation>{children}</UserNavigation>
      </SidebarProvider>
    </SessionContext>
  );
}

function UserNavigation({ children }: { children: ReactNode }) {
  const { session, signOut } = useUserSession();
  const actions = useActions();
  const pathname = usePathname();
  const { state, isMobile, openMobile, setOpenMobile } = useSidebar();
  const { theme, setTheme } = useTheme();
  const page = userNavigation.find((item) => isUserPage(pathname, item.href));
  return (
    <>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton asChild size="lg" tooltip="用户中心">
                <Link href="/" onClick={() => setOpenMobile(false)} aria-label="Codex2API 用户中心">
                  <UserRound />
                  <span className="grid text-left leading-tight">
                    <span className="font-semibold">Codex2API</span>
                    <span className="text-xs text-muted-foreground">用户中心</span>
                  </span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarHeader>
        <SidebarContent>
          <nav aria-label="用户导航">
            {["工作台", "订阅服务", "账户"].map((group) => (
              <SidebarGroup key={group}>
                <SidebarGroupLabel>{group}</SidebarGroupLabel>
                <SidebarGroupContent>
                  <SidebarMenu>
                    {userNavigation
                      .filter((item) => item.group === group)
                      .map((item) => (
                        <SidebarMenuItem key={item.href}>
                          <SidebarMenuButton
                            asChild
                            isActive={isUserPage(pathname, item.href)}
                            tooltip={item.label}
                          >
                            <Link
                              href={item.href}
                              aria-current={isUserPage(pathname, item.href) ? "page" : undefined}
                              onClick={() => setOpenMobile(false)}
                            >
                              <item.icon />
                              <span>{item.label}</span>
                            </Link>
                          </SidebarMenuButton>
                        </SidebarMenuItem>
                      ))}
                  </SidebarMenu>
                </SidebarGroupContent>
              </SidebarGroup>
            ))}
          </nav>
        </SidebarContent>
        <SidebarFooter>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton
                tooltip="退出登录"
                disabled={actions.isBusy("logout")}
                onClick={() =>
                  void actions.run(
                    "logout",
                    async () => {
                      await request("/logout", { method: "POST" });
                      signOut();
                    },
                    { success: "已退出登录" },
                  )
                }
              >
                <LogOut />
                <span>{session.user.username} · 退出登录</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarFooter>
        <SidebarRail />
      </Sidebar>
      <SidebarInset className="min-w-0">
        <header className="flex h-12 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger
            className="-ml-1"
            aria-label={
              isMobile
                ? openMobile
                  ? "关闭导航"
                  : "打开导航"
                : state === "expanded"
                  ? "收窄导航"
                  : "展开导航"
            }
            aria-expanded={isMobile ? openMobile : state === "expanded"}
          />
          <Separator
            orientation="vertical"
            className="mr-2 data-vertical:h-4 data-vertical:self-auto"
          />
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem className="hidden md:block">
                <BreadcrumbLink asChild>
                  <Link href="/">用户中心</Link>
                </BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator className="hidden md:block" />
              <BreadcrumbItem>
                <BreadcrumbPage>{page?.label ?? "用户中心"}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>
          <div className="ml-auto flex items-center gap-1">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button type="button" variant="ghost" size="icon-sm" aria-label="主题设置">
                  {theme === "dark" ? <Moon /> : theme === "light" ? <Sun /> : <Monitor />}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>外观模式</DropdownMenuLabel>
                <DropdownMenuRadioGroup value={theme ?? "system"} onValueChange={setTheme}>
                  {[
                    { value: "light", label: "浅色", icon: Sun },
                    { value: "dark", label: "深色", icon: Moon },
                    { value: "system", label: "跟随系统", icon: Monitor },
                  ].map(({ value, label, icon: Icon }) => (
                    <DropdownMenuRadioItem key={value} value={value}>
                      <Icon />
                      {label}
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
                <DropdownMenuSeparator />
                <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">
                  仅在当前浏览器生效
                </DropdownMenuLabel>
              </DropdownMenuContent>
            </DropdownMenu>
            <Button asChild variant="ghost" size="icon-sm">
              <Link href="/profile/" aria-label={`用户信息：${session.user.username}`}>
                <Avatar className="size-7">
                  <AvatarFallback>{session.user.name.slice(0, 1).toUpperCase()}</AvatarFallback>
                </Avatar>
              </Link>
            </Button>
          </div>
        </header>
        <div className="flex min-w-0 flex-1 flex-col gap-4 p-4">
          <div className="min-w-0 space-y-4">{children}</div>
        </div>
      </SidebarInset>
    </>
  );
}
