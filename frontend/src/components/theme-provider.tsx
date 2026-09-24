"use client";

import { useEffect, type ReactNode } from "react";
import { ThemeProvider as NextThemeProvider } from "next-themes";

export function ThemeProvider({ children }: { children: ReactNode }) {
  useEffect(() => {
    // Remove the retired browser preference, including during a development reload.
    document.documentElement.removeAttribute("data-accent");
    try {
      window.localStorage.removeItem("codex2api-accent");
    } catch {
      // Appearance still works when browser storage is unavailable.
    }
  }, []);

  return (
    <NextThemeProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      storageKey="codex2api-theme"
    >
      {children}
    </NextThemeProvider>
  );
}
