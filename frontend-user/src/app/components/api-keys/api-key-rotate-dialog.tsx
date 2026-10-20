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
import { Spinner } from "@/components/ui/spinner";

export function ApiKeyRotateDialog({ state }: { state: ReturnType<typeof useApiKeysPage> }) {
  const { rotating, setRotating, rotatingBusy, rotate, resource } = state;
  return (
    <Dialog
      open={!!rotating}
      onOpenChange={(open) => {
        if (!open && !rotatingBusy) setRotating(undefined);
      }}
    >
      <DialogContent
        showCloseButton={!rotatingBusy}
        onEscapeKeyDown={(event) => {
          if (rotatingBusy) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (rotatingBusy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>轮换密钥</DialogTitle>
          <DialogDescription>
            为“{rotating?.name}”生成新密钥后，旧密钥立即失效；名称、API
            组和用量记录保持不变。新密钥仅显示一次。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={rotatingBusy}
            onClick={() => setRotating(undefined)}
          >
            取消
          </Button>
          <Button type="button" disabled={rotatingBusy || !resource.ready} onClick={rotate}>
            {rotatingBusy && <Spinner />}确认轮换
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
