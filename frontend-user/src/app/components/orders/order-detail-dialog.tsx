"use client";
import { OrderDetailDialogProps, useOrderDetailDialog } from "@/app/data/orders";
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cents, orderKinds, orderStatuses, orderTime } from "@/lib/orders";

export function OrderDetailDialog({
  selected,
  busy,
  setSelected,
  effective,
  resource,
  actions,
  act,
}: OrderDetailDialogProps) {
  const { handleClick, handleClick2 } = useOrderDetailDialog({ actions, act });
  return (
    <Dialog
      open={!!selected}
      onOpenChange={(open) => {
        if (!open && !busy) setSelected(undefined);
      }}
    >
      <DialogContent className="flex max-h-[90dvh] flex-col sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>订单明细</DialogTitle>
          <DialogDescription>
            {selected
              ? `${selected.plan_name} · ${orderKinds[selected.kind]}`
              : "订阅订单及费用明细"}
          </DialogDescription>
        </DialogHeader>
        <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
          <div className="space-y-4">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
              <dt className="text-muted-foreground">订单号</dt>
              <dd className="break-all">{selected?.id ?? "—"}</dd>

              <dt className="text-muted-foreground">状态</dt>
              <dd>{effective ? orderStatuses[effective] : "—"}</dd>
              <dt className="text-muted-foreground">原套餐</dt>
              <dd>{selected?.previous_plan_name ?? "—"}</dd>
              <dt className="text-muted-foreground">新套餐售价</dt>
              <dd>
                {selected
                  ? `${cents(selected.unit_price_cents)} / ${selected.duration_days} 天`
                  : "—"}
              </dd>
              <dt className="text-muted-foreground">生效期限</dt>
              <dd>
                {selected
                  ? selected.kind === "purchase" && effective !== "paid"
                    ? `支付后 ${selected.duration_days} 天`
                    : `${orderTime(selected.period_start_ms)} 至 ${orderTime(selected.period_end_ms)}`
                  : "—"}
              </dd>
              <dt className="text-muted-foreground">
                {selected?.kind === "upgrade" ? "新套餐剩余费用" : "套餐费用"}
              </dt>
              <dd>{selected ? cents(selected.gross_cents) : "—"}</dd>
              <dt className="text-muted-foreground">原订阅剩余抵扣</dt>
              <dd>{selected ? cents(selected.credit_cents) : "—"}</dd>
              <dt className="text-muted-foreground">支付方式</dt>
              <dd>钱包余额（USD）</dd>
              <dt className="text-muted-foreground">优惠券</dt>
              <dd>{selected?.coupon_code ?? "未使用"}</dd>
              <dt className="text-muted-foreground">优惠金额</dt>
              <dd>{selected ? cents(selected.discount_cents) : "—"}</dd>
              <dt className="font-medium">实际应付（USD）</dt>
              <dd className="font-semibold">{selected ? cents(selected.amount_cents) : "—"}</dd>
              <dt className="text-muted-foreground">支付后余额</dt>
              <dd>{selected?.balance_cents != null ? cents(selected.balance_cents) : "—"}</dd>
              <dt className="text-muted-foreground">创建时间</dt>
              <dd>{selected ? orderTime(selected.created_at_ms) : "—"}</dd>
              <dt className="text-muted-foreground">报价有效至</dt>
              <dd>{selected ? orderTime(selected.quote_expires_at_ms) : "—"}</dd>
              <dt className="text-muted-foreground">支付时间</dt>
              <dd>{orderTime(selected?.paid_at_ms ?? null)}</dd>
              <dt className="text-muted-foreground">取消时间</dt>
              <dd>{orderTime(selected?.cancelled_at_ms ?? null)}</dd>
              <dt className="text-muted-foreground">取消原因</dt>
              <dd>{selected?.cancel_reason ?? "—"}</dd>
            </dl>
            <Table aria-label="剩余订阅抵扣明细">
              <TableHeader>
                <TableRow>
                  <TableHead>原有效期</TableHead>
                  <TableHead>原售价</TableHead>
                  <TableHead>抵扣（USD）</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {selected?.pricing.credit_periods.map((period, index) => (
                  <TableRow key={index}>
                    <TableCell>
                      {orderTime(period.starts_at_ms)} 至 {orderTime(period.ends_at_ms)}
                    </TableCell>
                    <TableCell>
                      {cents(period.unit_price_cents)} / {period.duration_days} 天
                    </TableCell>
                    <TableCell>{cents(period.credit_cents)}</TableCell>
                  </TableRow>
                ))}
                {selected?.pricing.credit_periods.length === 0 && (
                  <TableRow>
                    <TableCell colSpan={3}>此订单无剩余订阅抵扣</TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
            <p className="text-xs text-muted-foreground">
              升级按确认的预览时刻的剩余时长计价，各有效期段四舍五入到美分；升级不延长原到期时间。
            </p>
          </div>
        </ScrollArea>
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={() => setSelected(undefined)}>
            关闭
          </Button>
          {effective === "pending" && (
            <Button
              variant="outline"
              disabled={busy || !resource.ready}
              onClick={() => handleClick()}
            >
              取消订单
            </Button>
          )}
          {effective === "pending" && (
            <Button disabled={busy || !resource.ready} onClick={() => handleClick2()}>
              确认支付 {selected ? cents(selected.amount_cents) : ""}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
