"use client";
import { useSidebar } from "@/components/ui/sidebar";
import { useActions, useErrorToast } from "@/lib/actions";
import { ApiError, request, setSessionCsrf, type Session } from "@/lib/api";
import { isUserPage, userNavigation } from "@/lib/navigation";
import { useTheme } from "next-themes";
import { usePathname } from "next/navigation";
import { createContext, useCallback, useContext, useEffect, useId, useState } from "react";

export function useUserShell() {
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
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
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
    );
  return {
    signOut,
    session,
    error,
    setError,
    setRetry,
    username,
    setUsername,
    password,
    setPassword,
    actions,
    id,
    handleSubmit,
  } as const;
}

export const SessionContext = createContext<{ session: Session; signOut: () => void } | undefined>(
  undefined,
);

export function useUserSession() {
  const context = useContext(SessionContext);
  if (!context) throw new Error("用户页面必须位于用户会话布局中");
  return context;
}

export function useUserNavigation() {
  const { session, signOut } = useUserSession();
  const actions = useActions();
  const pathname = usePathname();
  const { state, isMobile, openMobile, setOpenMobile } = useSidebar();
  const { theme, setTheme } = useTheme();
  const page = userNavigation.find((item) => isUserPage(pathname, item.href));
  const handleClick = () =>
    void actions.run(
      "logout",
      async () => {
        await request("/logout", { method: "POST" });
        signOut();
      },
      { success: "已退出登录" },
    );
  return {
    session,
    actions,
    pathname,
    state,
    isMobile,
    openMobile,
    setOpenMobile,
    theme,
    setTheme,
    page,
    handleClick,
  } as const;
}
