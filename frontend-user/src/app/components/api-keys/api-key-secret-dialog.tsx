"use client";
import type { useApiKeysPage } from "@/app/data/api-keys";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Copy } from "lucide-react";

export function ApiKeySecretDialog({ state }: { state: ReturnType<typeof useApiKeysPage> }) {
  const { created, setCreated, secretRef, copySecret } = state;
  return (
    <Dialog
      open={!!created}
      onOpenChange={(open) => {
        if (!open) setCreated(undefined);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>密钥已创建</DialogTitle>
          <DialogDescription>完整密钥仅显示一次，请复制并妥善保存。</DialogDescription>
        </DialogHeader>
        <p className="text-sm">
          {created?.key.name} · {created?.key.group_name}
        </p>
        <p ref={secretRef} className="select-all break-all font-mono text-sm">
          {created?.secret}
        </p>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={copySecret}>
            <Copy />
            复制密钥
          </Button>
          <Button type="button" onClick={() => setCreated(undefined)}>
            已保存，关闭
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
