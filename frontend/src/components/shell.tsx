"use client";
import { ScrollArea } from "@/components/ui/scroll-area";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { ArrowRightLeft, LogOut, Monitor, Moon, Search, Sun } from "lucide-react";
import { useTheme } from "next-themes";
import { ApiError, request, setCsrf } from "@/lib/http";
import {
  useActions,
  useErrorToast,
  dismissConfirmation,
  restoreConfirmationFocus,
} from "@/lib/actions";
import { isCurrentPage, navigation } from "./navigation";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Spinner } from "@/components/ui/spinner";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Separator } from "@/components/ui/separator";
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
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

type Session = {
  authenticated: boolean;
  username: string;
  csrf_token: string;
  app_version: string;
  codex_cli_version: string;
};
const signedOut: Session = {
  authenticated: false,
  username: "",
  csrf_token: "",
  app_version: "",
  codex_cli_version: "",
};
async function readSession(signal?: AbortSignal): Promise<Session> {
  try {
    const value = await request<Session>("/session", { signal });
    if (
      value?.authenticated !== true ||
      typeof value.username !== "string" ||
      typeof value.csrf_token !== "string"
    )
      throw new ApiError(200, "invalid_response", "无法读取有效的管理员会话，请重试。");
    return value;
  } catch (reason) {
    if (reason instanceof ApiError && reason.status === 401 && reason.code === "unauthorized")
      return signedOut;
    throw reason;
  }
}
const Context = createContext<{ session?: Session; refresh: () => Promise<void> }>({
  refresh: async () => {},
});
export const useSession = () => useContext(Context);
const subscribeMounted = () => () => {};

export function AppShell({ children }: { children: ReactNode }) {
  const pathname = usePathname();
  const actions = useActions();
  useEffect(() => {
    dismissConfirmation();
  }, [pathname]);
  return (
    <>
      {pathname.startsWith("/authorize") ? (
        children
      ) : (
        <SidebarProvider>
          <AdminShell>{children}</AdminShell>
        </SidebarProvider>
      )}
      <AlertDialog
        open={Boolean(actions.pending)}
        onOpenChange={(open) => {
          if (!open) actions.dismiss();
        }}
      >
        <AlertDialogContent
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            restoreConfirmationFocus();
          }}
        >
          <AlertDialogHeader>
            <AlertDialogTitle>确认操作</AlertDialogTitle>
            <AlertDialogDescription>{actions.pending?.confirm}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel
              disabled={Boolean(actions.pending && actions.isBusy(actions.pending.key))}
            >
              取消
            </AlertDialogCancel>
            <AlertDialogAction
              variant={actions.pending?.danger ? "destructive" : "default"}
              disabled={Boolean(actions.pending && actions.isBusy(actions.pending.key))}
              onClick={(event) => {
                event.preventDefault();
                actions.confirm();
              }}
            >
              确认
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

function AdminShell({ children }: { children: ReactNode }) {
  const id = useId();
  const actions = useActions();
  const [session, setSession] = useState<Session>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const pathname = usePathname();
  const router = useRouter();
  const { state, isMobile, openMobile, setOpenMobile } = useSidebar();
  const { theme, setTheme } = useTheme();
  const mounted = useSyncExternalStore(
    subscribeMounted,
    () => true,
    () => false,
  );
  const page = navigation.find((item) => isCurrentPage(pathname, item.href));
  useErrorToast(error);
  const acceptSession = useCallback((value: Session) => {
    setSession(value);
    setCsrf(value.csrf_token);
    setError("");
    if (!value.authenticated) dismissConfirmation();
  }, []);
  async function refresh() {
    acceptSession(await readSession());
  }
  useEffect(() => {
    const controller = new AbortController();
    readSession(controller.signal)
      .then((value) => {
        if (!controller.signal.aborted) acceptSession(value);
      })
      .catch((reason) => {
        if (!controller.signal.aborted)
          setError(reason instanceof Error ? reason.message : "无法读取管理员会话");
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    const expired = () => {
      setSession(signedOut);
      setCsrf("");
      dismissConfirmation();
    };
    window.addEventListener("admin-session-expired", expired);
    return () => {
      controller.abort();
      window.removeEventListener("admin-session-expired", expired);
    };
  }, [acceptSession]);
  useEffect(() => {
    if (!session?.authenticated) return;
    const shortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSearchOpen((open) => !open);
      }
    };
    document.addEventListener("keydown", shortcut);
    return () => document.removeEventListener("keydown", shortcut);
  }, [session?.authenticated]);
  if (loading)
    return (
      <main className="flex min-h-svh w-full items-center justify-center">
        <Spinner aria-label="正在加载" />
      </main>
    );
  if (!session)
    return (
      <main className="flex min-h-svh w-full items-center justify-center">
        <Button
          variant="outline"
          disabled={actions.isBusy("admin-session-retry")}
          onClick={() => void actions.run("admin-session-retry", () => refresh(), { success: "" })}
        >
          {actions.isBusy("admin-session-retry") && <Spinner />}
          重新连接
        </Button>
      </main>
    );
  if (!session.authenticated)
    return (
      <Context value={{ session, refresh }}>
        <main className="flex min-h-svh w-full items-center justify-center p-4">
          <Card className="w-full max-w-sm">
            <CardHeader>
              <CardTitle role="heading" aria-level={1}>
                管理登录
              </CardTitle>
              <CardDescription>Codex2API · 供应账户与虚拟账户管理</CardDescription>
            </CardHeader>
            <CardContent>
              <form
                noValidate
                onSubmit={(event) =>
                  actions.submit(
                    event,
                    "admin-login",
                    async () => {
                      const value = await request<Session>("/login", {
                        method: "POST",
                        body: { username, password },
                      });
                      setSession(value);
                      setCsrf(value.csrf_token);
                      setPassword("");
                      setError("");
                    },
                    "登录成功",
                  )
                }
              >
                <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
                  <FieldSet disabled={actions.isBusy("admin-login")}>
                    <FieldGroup className="gap-4">
                      <Field>
                        <FieldLabel htmlFor={`${id}-username`}>用户名</FieldLabel>
                        <Input
                          id={`${id}-username`}
                          value={username}
                          onChange={(event) => setUsername(event.target.value)}
                          autoComplete="username"
                          required
                        />
                      </Field>
                      <Field>
                        <FieldLabel htmlFor={`${id}-password`}>密码</FieldLabel>
                        <Input
                          id={`${id}-password`}
                          type="password"
                          value={password}
                          onChange={(event) => setPassword(event.target.value)}
                          autoComplete="current-password"
                          required
                        />
                      </Field>
                      <Button type="submit" disabled={actions.isBusy("admin-login")}>
                        {actions.isBusy("admin-login") && <Spinner />}登录
                      </Button>
                    </FieldGroup>
                  </FieldSet>
                </ScrollArea>
              </form>
            </CardContent>
          </Card>
        </main>
      </Context>
    );
  // Official shadcn sidebar-07 structure with the application's real routes.
  return (
    <Context value={{ session, refresh }}>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton asChild size="lg">
                <Link href="/" aria-label="Codex2API 管理首页">
                  <ArrowRightLeft />
                  <span>Codex2API</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarHeader>
        <SidebarContent>
          <nav aria-label="主导航">
            {["工作台", "账户管理", "服务配置"].map((group) => (
              <SidebarGroup key={group}>
                <SidebarGroupLabel>{group}</SidebarGroupLabel>
                <SidebarGroupContent>
                  <SidebarMenu>
                    {navigation
                      .filter((item) => item.group === group)
                      .map((item) => (
                        <SidebarMenuItem key={item.href}>
                          <SidebarMenuButton
                            asChild
                            isActive={isCurrentPage(pathname, item.href)}
                            tooltip={item.label}
                          >
                            <Link
                              href={item.href}
                              aria-current={isCurrentPage(pathname, item.href) ? "page" : undefined}
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
                onClick={() =>
                  void actions.run(
                    "admin-logout",
                    async () => {
                      await request("/logout", { method: "POST" });
                      setSession(signedOut);
                      setCsrf("");
                      dismissConfirmation();
                    },
                    { success: "已退出登录" },
                  )
                }
                disabled={actions.isBusy("admin-logout")}
                tooltip="退出登录"
              >
                <LogOut />
                <span>{session.username} · 退出登录</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarFooter>
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
                : state === "collapsed"
                  ? "展开导航"
                  : "收窄导航"
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
                  <Link href="/">管理工作台</Link>
                </BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator className="hidden md:block" />
              <BreadcrumbItem>
                <BreadcrumbPage>{page?.label ?? "管理"}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>
          <div className="ml-auto flex items-center gap-1">
            <div
              aria-label="版本信息"
              className="mr-2 flex flex-col items-end gap-0.5 whitespace-nowrap text-xs text-muted-foreground sm:flex-row sm:gap-3"
            >
              <span>Codex CLI {session.codex_cli_version || "—"}</span>
              <span>Codex2API {session.app_version || "—"}</span>
            </div>
            <Dialog open={searchOpen} onOpenChange={setSearchOpen}>
              <DialogTrigger asChild>
                <Button variant="ghost" size="icon-sm" aria-label="搜索页面">
                  <Search />
                </Button>
              </DialogTrigger>
              <DialogContent className="p-0" showCloseButton={false}>
                <DialogHeader className="sr-only">
                  <DialogTitle>搜索页面</DialogTitle>
                  <DialogDescription>搜索管理页面，使用方向键选择并按回车打开。</DialogDescription>
                </DialogHeader>
                <Command>
                  <CommandInput aria-label="搜索管理页面" placeholder="搜索页面…" />
                  <CommandList>
                    <CommandEmpty>没有匹配的页面</CommandEmpty>
                    {["工作台", "账户管理", "服务配置"].map((group) => (
                      <CommandGroup key={group} heading={group}>
                        {navigation
                          .filter((item) => item.group === group)
                          .map((item) => (
                            <CommandItem
                              key={item.href}
                              value={`${item.label} ${item.keywords}`}
                              onSelect={() => {
                                setSearchOpen(false);
                                router.push(item.href);
                              }}
                            >
                              <item.icon />
                              {item.label}
                            </CommandItem>
                          ))}
                      </CommandGroup>
                    ))}
                  </CommandList>
                </Command>
              </DialogContent>
            </Dialog>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label="主题设置"
                  disabled={!mounted}
                >
                  {mounted && theme === "dark" ? (
                    <Moon />
                  ) : mounted && theme === "light" ? (
                    <Sun />
                  ) : (
                    <Monitor />
                  )}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>外观模式</DropdownMenuLabel>
                <DropdownMenuRadioGroup
                  value={mounted ? (theme ?? "system") : "system"}
                  onValueChange={setTheme}
                >
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
            <Avatar className="ml-1 size-7" aria-label={`管理员 ${session.username}`}>
              <AvatarFallback>{session.username.slice(0, 1).toUpperCase()}</AvatarFallback>
            </Avatar>
          </div>
        </header>
        <div className="flex min-w-0 flex-1 flex-col gap-4 p-4">
          <div className="min-w-0 space-y-4">{children}</div>
        </div>
      </SidebarInset>
    </Context>
  );
}
