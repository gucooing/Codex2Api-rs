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

export function ApiKeyRevokeDialog({ state }: { state: ReturnType<typeof useApiKeysPage> }) {
  const { revoking, setRevoking, revokingBusy, revoke, resource } = state;
  return (
    <Dialog
      open={!!revoking}
      onOpenChange={(open) => {
        if (!open && !revokingBusy) setRevoking(undefined);
      }}
    >
      <DialogContent
        showCloseButton={!revokingBusy}
        onEscapeKeyDown={(event) => {
          if (revokingBusy) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (revokingBusy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>撤销密钥</DialogTitle>
          <DialogDescription>
            撤销“{revoking?.name}”后，此密钥无法继续调用，也不能恢复。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={revokingBusy}
            onClick={() => setRevoking(undefined)}
          >
            取消
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={revokingBusy || !resource.ready}
            onClick={revoke}
          >
            {revokingBusy && <Spinner />}确认撤销
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
