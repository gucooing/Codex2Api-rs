"use client";
import { SecurityEditor } from "@/app/components/settings/security-editor";
import { useSecurity } from "@/app/data/settings";

export function Security() {
  const { resource } = useSecurity();
  return (
    <SecurityEditor
      username={resource.data?.username ?? ""}
      disabled={!resource.ready}
      refreshing={resource.refreshing}
      onReload={resource.reload}
    />
  );
}
