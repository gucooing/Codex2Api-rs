import type { Metadata } from "next";
import { AppShell } from "@/components/shell";
import "./styles.css";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";
import { ThemeProvider } from "@/components/theme-provider";
import {
  CircleCheckIcon,
  OctagonXIcon,
  TriangleAlertIcon,
  InfoIcon,
  Loader2Icon,
} from "lucide-react";

export const metadata: Metadata = {
  title: "Codex2API 管理",
  description: "供应账户与虚拟账户管理",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="zh-CN" suppressHydrationWarning>
      <body>
        <ThemeProvider>
          <TooltipProvider>
            <AppShell>{children}</AppShell>
            <Toaster
              position="top-right"
              duration={3200}
              closeButton
              richColors={false}
              icons={{
                success: <CircleCheckIcon className="size-4 text-green-600 dark:text-green-400" />,
                error: <OctagonXIcon className="size-4 text-red-600 dark:text-red-400" />,
                warning: (
                  <TriangleAlertIcon className="size-4 text-yellow-500 dark:text-yellow-400" />
                ),
                info: <InfoIcon className="size-4" />,
                loading: <Loader2Icon className="size-4 animate-spin" />,
              }}
            />
          </TooltipProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
