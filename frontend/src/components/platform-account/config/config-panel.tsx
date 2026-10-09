"use client";
import { ConfigForm } from "@/components/platform-account/config/config-form";
import { useConfigPanel } from "@/lib/platform-account/config";

export function ConfigPanel({ id, group }: { id: string; group: string }) {
  const { resource, category, items } = useConfigPanel({ id, group });
  return (
    <ConfigForm
      key={`${id}:${group}`}
      id={id}
      group={group}
      title={category?.label ?? "账户设置"}
      items={items}
      disabled={
        !resource.ready ||
        items.some(
          (item) =>
            item.readonly || !resource.data?.items.some((stored) => stored.key === item.key),
        )
      }
      onSaved={resource.reload}
    />
  );
}
