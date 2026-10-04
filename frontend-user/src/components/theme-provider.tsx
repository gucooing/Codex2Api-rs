"use client";
import type { ReactNode } from "react";
import { ThemeProvider as NextThemeProvider } from "next-themes";
export function ThemeProvider({ children }: { children: ReactNode }) {
  return (
    <NextThemeProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      storageKey="codex2api-user-theme"
    >
      {children}
    </NextThemeProvider>
  );
}
