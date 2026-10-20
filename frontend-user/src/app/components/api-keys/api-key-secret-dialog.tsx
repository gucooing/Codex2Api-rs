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
  const { secret, setSecret, secretRef, copySecret } = state;
  const rotated = secret?.action === "rotated";
  return (
    <Dialog
      open={!!secret}
      onOpenChange={(open) => {
        if (!open) setSecret(undefined);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{rotated ? "密钥已轮换" : "密钥已创建"}</DialogTitle>
          <DialogDescription>
            {rotated ? "旧密钥已失效。" : ""}完整密钥仅显示一次，请复制并妥善保存。
          </DialogDescription>
        </DialogHeader>
        <p className="text-sm">
          {secret?.key.name} · {secret?.key.group_name}
        </p>
        <p ref={secretRef} className="select-all break-all font-mono text-sm">
          {secret?.secret}
        </p>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={copySecret}>
            <Copy />
            复制密钥
          </Button>
          <Button type="button" onClick={() => setSecret(undefined)}>
            已保存，关闭
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
