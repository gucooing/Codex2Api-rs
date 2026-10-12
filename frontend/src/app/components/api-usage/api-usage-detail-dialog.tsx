"use client";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { date } from "@/lib/format";
import { apiBillingStatus, apiCost, apiUsageStatus, type ApiUsage } from "@/lib/metered-api";

export function ApiUsageDetailDialog({
  selected,
  onClose,
}: {
  selected: ApiUsage | undefined;
  onClose: () => void;
}) {
  return (
    <Dialog
      open={!!selected}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="flex max-h-[90dvh] flex-col">
        <DialogHeader>
          <DialogTitle>API 请求详情</DialogTitle>
          <DialogDescription>{selected?.id ?? "请求记录"}</DialogDescription>
        </DialogHeader>
        <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[65dvh]">
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-3 pr-3 text-sm">
            <dt className="text-muted-foreground">用户 / 密钥</dt>
            <dd>
              {selected?.username ?? "—"} / {selected?.key_name ?? "—"}
            </dd>
            <dt className="text-muted-foreground">API 组</dt>
            <dd>{selected?.group_name ?? "—"}</dd>
            <dt className="text-muted-foreground">模型</dt>
            <dd className="break-all">{selected?.model ?? "—"}</dd>
            <dt className="text-muted-foreground">响应模型</dt>
            <dd className="break-all">{selected?.actual_model ?? "—"}</dd>
            <dt className="text-muted-foreground">接口</dt>
            <dd>{selected?.endpoint ?? "—"}</dd>
            <dt className="text-muted-foreground">请求时间</dt>
            <dd>{date(selected?.requested_at_ms)}</dd>
            <dt className="text-muted-foreground">完成时间</dt>
            <dd>{date(selected?.completed_at_ms)}</dd>
            <dt className="text-muted-foreground">状态 / HTTP</dt>
            <dd>
              {selected ? apiUsageStatus(selected.status) : "—"} / {selected?.http_status ?? "—"}
            </dd>
            <dt className="text-muted-foreground">输入 / 输出</dt>
            <dd>
              {selected?.input_tokens ?? "—"} / {selected?.output_tokens ?? "—"}
            </dd>
            <dt className="text-muted-foreground">缓存读取 / 写入</dt>
            <dd>
              {selected?.cached_tokens ?? "—"} / {selected?.cache_write_tokens ?? "—"}
            </dd>
            <dt className="text-muted-foreground">推理 Token</dt>
            <dd>{selected?.reasoning_tokens ?? "—"}</dd>
            <dt className="text-muted-foreground">图像数量 / 分辨率</dt>
            <dd>
              {selected?.image_count ?? "—"} / {selected?.image_size ?? "—"}
            </dd>
            <dt className="text-muted-foreground">计费倍率</dt>
            <dd>{selected?.multiplier ?? "—"}×</dd>
            <dt className="text-muted-foreground">费用 / 结算状态</dt>
            <dd>
              {selected ? apiCost(selected.cost_nano_usd) : "—"} /{" "}
              {selected ? apiBillingStatus(selected.billing_status) : "—"}
            </dd>
            <dt className="text-muted-foreground">钱包扣款</dt>
            <dd>
              {selected?.wallet_debit_cents == null
                ? "—"
                : `$${(selected.wallet_debit_cents / 100).toFixed(2)}`}
            </dd>
          </dl>
        </ScrollArea>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            关闭
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
