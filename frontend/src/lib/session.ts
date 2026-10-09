"use client";
import { useSidebar } from "@/components/ui/sidebar";
import { dismissConfirmation, toastError, useActions, useErrorToast } from "@/lib/actions";
import { ApiError, refreshSession, request, setCsrf } from "@/lib/http";
import { isCurrentPage, navigation } from "@/lib/navigation";
import { keepSessionAlive } from "@/lib/session-refresh";
import { useTheme } from "next-themes";
import { usePathname, useRouter } from "next/navigation";
import { useCallback, useEffect, useId, useState, useSyncExternalStore } from "react";

export type Session = {
  authenticated: boolean;
  username: string;
  csrf_token: string;
  app_version: string;
  codex_cli_version: string;
  grok_build_version: string;
};

export const signedOut: Session = {
  authenticated: false,
  username: "",
  csrf_token: "",
  app_version: "",
  codex_cli_version: "",
  grok_build_version: "",
};

export async function readSession(signal?: AbortSignal): Promise<Session> {
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

export const subscribeMounted = () => () => {};

export function useAdminShell() {
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
    return keepSessionAlive(() => refreshSession(), toastError);
  }, [session?.authenticated]);
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
  const handleClick = () =>
    void actions.run("admin-session-retry", () => refresh(), { success: "" });
  const handleSubmit2 = (event: React.SubmitEvent<HTMLFormElement>) =>
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
    );
  const handleClick3 = () =>
    void actions.run(
      "admin-logout",
      async () => {
        await request("/logout", { method: "POST" });
        setSession(signedOut);
        setCsrf("");
        dismissConfirmation();
      },
      { success: "已退出登录" },
    );
  return {
    refresh,
    id,
    actions,
    session,
    loading,
    username,
    setUsername,
    password,
    setPassword,
    searchOpen,
    setSearchOpen,
    pathname,
    router,
    state,
    isMobile,
    openMobile,
    setOpenMobile,
    theme,
    setTheme,
    mounted,
    page,
    handleClick,
    handleSubmit2,
    handleClick3,
  } as const;
}

export function useAppShell() {
  const pathname = usePathname();
  const actions = useActions();
  useEffect(() => {
    dismissConfirmation();
  }, [pathname]);

  return { actions } as const;
}
