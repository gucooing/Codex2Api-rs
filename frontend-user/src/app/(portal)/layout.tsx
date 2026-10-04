import type { ReactNode } from "react";
import { UserShell } from "@/components/user-shell";
export default function PortalLayout({ children }: { children: ReactNode }) {
  return <UserShell>{children}</UserShell>;
}
