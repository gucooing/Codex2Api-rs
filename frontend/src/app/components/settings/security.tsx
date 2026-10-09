"use client";
import { SecurityEditor } from "@/app/components/settings/security-editor";
import { useSecurity } from "@/app/data/settings";
import { Button } from "@/components/ui/button";

export function Security() {
  const { resource } = useSecurity();
  return (
    <>
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={resource.reload}
        disabled={resource.refreshing}
      >
        重新加载
      </Button>
      <SecurityEditor
        key={resource.data?.username ?? "unloaded"}
        username={resource.data?.username ?? ""}
        disabled={!resource.ready}
      />
    </>
  );
}
