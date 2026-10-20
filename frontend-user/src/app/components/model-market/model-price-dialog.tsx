"use client";
import { ModelPriceTable } from "./model-price-table";
import { ModelCallLimits } from "./model-call-limits";
import type { ModelMarketItem } from "@/app/data/model-market/pricing";
import { ModelPlatformIcon } from "@/components/model-platforms";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";
import type { ApiGroup } from "@/lib/api-access";
import { platformLabel } from "@/lib/model-platforms";

export function ModelPriceDialog({
  item,
  group,
  onClose,
}: {
  item: ModelMarketItem | undefined;
  group: ApiGroup | null | undefined;
  onClose: () => void;
}) {
  return (
    <Dialog
      open={!!item}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="flex max-h-[90dvh] flex-col sm:max-w-3xl">
        <DialogHeader className="pr-6">
          <DialogTitle className="flex items-center gap-2">
            <ModelPlatformIcon platform={item?.model.platform ?? ""} className="size-5 shrink-0" />
            <span className="min-w-0 break-all">{item?.model.model ?? "模型价格"}</span>
          </DialogTitle>
          <DialogDescription>
            {platformLabel(item?.model.platform ?? "")} · {group?.name ?? "基础价格"} ·{" "}
            {item?.model.kind === "image" ? "USD / 张" : "USD / 百万 Token"}
            {group && ` · 计费倍率 ${item?.model.multiplier}×`}
          </DialogDescription>
        </DialogHeader>
        <ScrollArea
          type="auto"
          className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[65dvh]"
        >
          <div className="min-w-0 space-y-4 pr-3 pb-3">
            <ModelPriceTable item={item} />
            {group && item && (
              <section className="space-y-2 border-t pt-3" aria-label="调用限制">
                <h3 className="text-sm font-medium">调用限制</h3>
                <ModelCallLimits model={item.model} />
              </section>
            )}
          </div>
          <ScrollBar orientation="horizontal" />
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}
