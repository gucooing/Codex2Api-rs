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
import { date, money } from "@/lib/format";
import type { AdminWalletEntry } from "@/lib/users";
import { walletSources } from "@/lib/wallet";
import type { Dispatch, SetStateAction } from "react";

type Props = {
  selected: AdminWalletEntry | undefined;
  setSelected: Dispatch<SetStateAction<AdminWalletEntry | undefined>>;
};

export function WalletEntryDialog({ selected, setSelected }: Props) {
  return (
    <Dialog
      open={!!selected}
      onOpenChange={(open) => {
        if (!open) setSelected(undefined);
      }}
    >
      <DialogContent className="flex max-h-[90dvh] flex-col">
        <DialogHeader>
          <DialogTitle>流水详情</DialogTitle>
          <DialogDescription>{selected?.username ?? "钱包流水"}</DialogDescription>
        </DialogHeader>
        <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 pr-3 text-sm">
            <dt className="text-muted-foreground">流水号</dt>
            <dd className="break-all">{selected?.id ?? "—"}</dd>
            <dt className="text-muted-foreground">用户</dt>
            <dd>{selected ? `${selected.username} · ${selected.user_name}` : "—"}</dd>
            <dt className="text-muted-foreground">来源</dt>
            <dd>{selected ? walletSources[selected.kind] : "—"}</dd>
            <dt className="text-muted-foreground">套餐</dt>
            <dd>{selected?.plan_name ?? "—"}</dd>
            <dt className="text-muted-foreground">关联订单</dt>
            <dd className="break-all">{selected?.order_id ?? "—"}</dd>
            <dt className="text-muted-foreground">前余额</dt>
            <dd>{selected ? money(selected.balance_before_cents / 100) : "—"} USD</dd>
            <dt className="text-muted-foreground">变动金额</dt>
            <dd>
              {selected
                ? `${selected.amount_cents > 0 ? "+" : ""}${money(selected.amount_cents / 100)}`
                : "—"}{" "}
              USD
            </dd>
            <dt className="text-muted-foreground">后余额</dt>
            <dd>{selected ? money(selected.balance_cents / 100) : "—"} USD</dd>
            <dt className="text-muted-foreground">时间</dt>
            <dd>{selected ? date(selected.created_at) : "—"}</dd>
            <dt className="text-muted-foreground">操作人</dt>
            <dd>{selected?.operator_name ?? "—"}</dd>
            <dt className="text-muted-foreground">原因</dt>
            <dd className="whitespace-pre-wrap break-words">{selected?.reason ?? "—"}</dd>
          </dl>
        </ScrollArea>
        <DialogFooter>
          <Button variant="outline" onClick={() => setSelected(undefined)}>
            关闭
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
