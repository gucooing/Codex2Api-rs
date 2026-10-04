import type { Metadata } from "next";
import { ThemeProvider } from "@/components/theme-provider";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import "./styles.css";
export const metadata: Metadata = { title: "Codex2API 用户中心", description: "个人订阅与钱包" };
export default function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="zh-CN" suppressHydrationWarning>
      {/* Browser extensions may add attributes to body before React hydrates it. */}
      <body suppressHydrationWarning>
        <ThemeProvider>
          <TooltipProvider>
            {children}
            <Toaster position="top-right" closeButton />
          </TooltipProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
