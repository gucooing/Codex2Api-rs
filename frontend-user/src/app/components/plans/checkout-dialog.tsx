"use client";
import { CheckoutDialogProps, useCheckoutDialog } from "@/app/data/plans";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { money } from "@/lib/api";
import { cents, orderKinds, orderStatuses, orderTime } from "@/lib/orders";
import Link from "next/link";

export function CheckoutDialog({
  selected,
  busy,
  setSelected,
  stage,
  status,
  detail,
  wallet,
  order,
  preview,
  walletBalance,
  id,
  payment,
  setPayment,
  coupon,
  setCoupon,
  plans,
  subscriptions,
  actions,
  loadPreview,
  dirty,
  previewExpired,
  setOrder,
  refresh,
  pay,
}: CheckoutDialogProps) {
  const { handleClick, handleClick2, handleClick3, handleClick4, handleClick5, handleClick6 } =
    useCheckoutDialog({
      selected,
      order,
      preview,
      payment,
      coupon,
      actions,
      loadPreview,
      setOrder,
      refresh,
      pay,
    });
  return (
    <Dialog
      open={!!selected}
      onOpenChange={(open) => {
        if (!open && !busy) setSelected(undefined);
      }}
    >
      <DialogContent className="flex max-h-[90dvh] flex-col sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {stage === 1 ? "订单预览" : stage === 3 ? "订单完成" : "订单支付"}
          </DialogTitle>
          <DialogDescription>
            {stage === 1
              ? "核对套餐、有效期和优惠，确认后才会创建订单。"
              : stage === 3
                ? "支付已完成，订阅权益已更新。"
                : status === "pending"
                  ? "订单已创建，请在有效期内完成支付。"
                  : "订单已结束，可关闭后重新选择套餐。"}
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-wrap gap-2" aria-label="结算步骤">
          {["预览并确认订单", "支付", "完成"].map((label, index) => (
            <Badge key={label} variant={stage === index + 1 ? "default" : "secondary"}>
              {index + 1} · {label}
            </Badge>
          ))}
        </div>
        <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-16rem)]">
          <div className="space-y-4 pr-3">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
              <dt className="text-muted-foreground">平台 / 套餐</dt>
              <dd>
                {selected?.provider_id === "chatgpt"
                  ? "ChatGPT"
                  : selected?.provider_id === "grok"
                    ? "Grok"
                    : selected?.provider_id}{" "}
                / {selected?.name ?? "—"}
              </dd>
              <dt className="text-muted-foreground">业务类型</dt>
              <dd>{detail ? orderKinds[detail.kind] : "—"}</dd>
              <dt className="text-muted-foreground">套餐售价</dt>
              <dd>
                {detail ? `${cents(detail.unit_price_cents)} / ${detail.duration_days} 天` : "—"}
              </dd>
              <dt className="text-muted-foreground">原套餐</dt>
              <dd>{detail?.previous_plan_name ?? "—"}</dd>
              <dt className="text-muted-foreground">有效期</dt>
              <dd>
                {detail
                  ? detail.kind === "purchase" && status !== "paid"
                    ? `支付成功起 ${detail.duration_days} 天；预计至 ${orderTime(detail.period_end_ms)}`
                    : `${detail.kind === "upgrade" && status !== "paid" ? "支付成功起" : orderTime(detail.period_start_ms)} 至 ${orderTime(detail.period_end_ms)}`
                  : "—"}
              </dd>
              <dt className="text-muted-foreground">
                {detail?.kind === "upgrade" ? "新套餐剩余费用" : "套餐费用"}
              </dt>
              <dd>{detail ? cents(detail.gross_cents) : "—"}</dd>
              <dt className="text-muted-foreground">原订阅剩余抵扣</dt>
              <dd>{detail ? cents(detail.credit_cents) : "—"}</dd>
              <dt className="text-muted-foreground">优惠券抵扣</dt>
              <dd>
                {detail ? cents(detail.discount_cents) : "—"}
                {detail?.coupon_code ? `（${detail.coupon_code}）` : ""}
              </dd>
              <dt className="font-medium">实际应付（USD）</dt>
              <dd className="font-semibold tabular-nums">
                {detail ? cents(detail.amount_cents) : "—"}
              </dd>
              <dt className="text-muted-foreground">钱包余额</dt>
              <dd>{wallet.data ? money(wallet.data.balance_usd) : "—"}</dd>
              <dt className="text-muted-foreground">{order ? "支付截止" : "预览有效至"}</dt>
              <dd>
                {order
                  ? orderTime(order.quote_expires_at_ms)
                  : preview
                    ? orderTime(preview.expires_at_ms)
                    : "—"}
              </dd>
              {order && (
                <>
                  <dt className="text-muted-foreground">订单号</dt>
                  <dd className="break-all">{order.id}</dd>
                  <dt className="text-muted-foreground">状态</dt>
                  <dd>{status && orderStatuses[status]}</dd>
                  <dt className="text-muted-foreground">支付方式</dt>
                  <dd>钱包余额（可用 {walletBalance}）</dd>
                  <dt className="text-muted-foreground">支付时间</dt>
                  <dd>{orderTime(order.paid_at_ms)}</dd>
                </>
              )}
            </dl>
            <p className="text-xs text-muted-foreground">
              {detail?.kind === "upgrade"
                ? "升级按本次预览时刻的剩余有效期计算差价，原到期时间不变。优惠券在原订阅抵扣后使用。"
                : detail?.kind === "renew"
                  ? "续订从当前到期时间开始延长有效期。"
                  : "购买有效期从实际支付成功时开始计算。"}
            </p>
            {stage === 1 && (
              <>
                <Field>
                  <FieldLabel htmlFor={`${id}-payment`}>支付方式</FieldLabel>
                  <Select value={payment} onValueChange={setPayment} disabled={busy}>
                    <SelectTrigger id={`${id}-payment`}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="wallet">钱包余额（可用 {walletBalance}）</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-coupon`}>优惠码（可选）</FieldLabel>
                  <div className="flex gap-2">
                    <Input
                      id={`${id}-coupon`}
                      value={coupon}
                      maxLength={64}
                      autoComplete="off"
                      disabled={busy}
                      onChange={(event) => setCoupon(event.target.value)}
                      placeholder="输入优惠码"
                    />
                    <Button
                      variant="outline"
                      disabled={
                        busy || !coupon.trim() || !selected || !plans.ready || !subscriptions.ready
                      }
                      onClick={() => handleClick()}
                    >
                      {actions.isBusy("apply-coupon") && <Spinner />}使用优惠码
                    </Button>
                  </div>
                  {preview?.coupon_code && (
                    <div className="flex items-center gap-2 text-xs">
                      <span className="text-muted-foreground">已使用 {preview.coupon_code}</span>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={busy || !selected || !plans.ready || !subscriptions.ready}
                        onClick={() => handleClick2()}
                      >
                        移除优惠码
                      </Button>
                    </div>
                  )}
                </Field>
                <p className="text-xs text-muted-foreground">
                  {dirty
                    ? "使用填写的优惠码，或移除已使用的优惠码后再确认。"
                    : previewExpired
                      ? "预览已过期，请刷新预览后确认。"
                      : "关闭预览不会创建订单。确认订单后，优惠券次数将保留至支付截止。"}
                </p>
              </>
            )}
            {order?.cancel_reason && (
              <p className="text-xs text-muted-foreground">{order.cancel_reason}</p>
            )}
          </div>
        </ScrollArea>
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={() => setSelected(undefined)}>
            {stage === 1 ? "返回" : "关闭"}
          </Button>
          {stage === 1 && (!preview || previewExpired) && (
            <Button
              variant="outline"
              disabled={busy || !selected || !plans.ready || !subscriptions.ready}
              onClick={() => handleClick3()}
            >
              {actions.isBusy("preview") && <Spinner />}刷新预览
            </Button>
          )}
          {stage === 1 && (
            <Button
              disabled={
                busy || !preview || dirty || previewExpired || !plans.ready || !subscriptions.ready
              }
              onClick={() => handleClick4()}
            >
              {actions.isBusy("confirm-order") && <Spinner />}确认订单
            </Button>
          )}
          {status === "pending" && (
            <>
              <Button variant="outline" disabled={busy} onClick={() => handleClick5()}>
                取消订单
              </Button>
              <Button disabled={busy || !wallet.ready} onClick={() => handleClick6()}>
                {actions.isBusy("pay-order") && <Spinner />}钱包支付{" "}
                {order ? cents(order.amount_cents) : ""}
              </Button>
            </>
          )}
          {stage === 3 && (
            <Button asChild>
              <Link href="/subscriptions/">查看订阅</Link>
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
