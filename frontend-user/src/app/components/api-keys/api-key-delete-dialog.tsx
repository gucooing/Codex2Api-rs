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

export function ApiKeyDeleteDialog({ state }: { state: ReturnType<typeof useApiKeysPage> }) {
  const { deleting, setDeleting, deletingBusy, remove, resource } = state;
  return (
    <Dialog
      open={!!deleting}
      onOpenChange={(open) => {
        if (!open && !deletingBusy) setDeleting(undefined);
      }}
    >
      <DialogContent
        showCloseButton={!deletingBusy}
        onEscapeKeyDown={(event) => {
          if (deletingBusy) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (deletingBusy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>删除密钥</DialogTitle>
          <DialogDescription>
            删除“{deleting?.name}”后，此密钥立即失效并从列表移除，不能恢复；已产生的用量记录保留。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={deletingBusy}
            onClick={() => setDeleting(undefined)}
          >
            取消
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={deletingBusy || !resource.ready}
            onClick={remove}
          >
            {deletingBusy && <Spinner />}确认删除
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
