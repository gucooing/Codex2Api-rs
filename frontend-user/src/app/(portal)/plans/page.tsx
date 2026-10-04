"use client";
import { useId, useState } from "react";
import Link from "next/link";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Badge } from "@/components/ui/badge";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { request, money, type Plan, type Subscription } from "@/lib/api";
import {
  cents,
  orderKinds,
  orderStatus,
  orderStatuses,
  orderTime,
  useOrderClock,
  type Order,
  type CheckoutPreview,
} from "@/lib/orders";
import { useResource } from "@/lib/resource";
import { useActions, useErrorToast } from "@/lib/actions";

export default function PlansPage() {
  const subscriptions = useResource<{ items: Subscription[] }>("/subscriptions", 0);
  const plans = useResource<{ items: Plan[] }>("/plans", 0);
  const wallet = useResource<{ balance_usd: string }>("/wallet", 0);
  const actions = useActions();
  const [selected, setSelected] = useState<Plan>();
  const [viewedId, setViewedId] = useState<string>();
  const viewed = plans.data?.items.find((plan) => plan.id === viewedId);
  const [preview, setPreview] = useState<CheckoutPreview>();
  const [order, setOrder] = useState<Order>();
  const [coupon, setCoupon] = useState("");
  const [payment, setPayment] = useState("wallet");
  const id = useId();
  const now = useOrderClock();
  const refresh = () => {
    subscriptions.reload();
    plans.reload();
    wallet.reload();
  };
  useErrorToast(subscriptions.error);
  useErrorToast(plans.error);
  useErrorToast(wallet.error);
  const status = order ? orderStatus(order, now) : undefined;
  const stage = !order ? 1 : status === "paid" ? 3 : 2;
  const busy = [
    "preview",
    "apply-coupon",
    "remove-coupon",
    "confirm-order",
    "pay-order",
    "cancel-order",
  ].some(actions.isBusy);
  const detail = order ?? preview;
  const dirty =
    !!preview &&
    (coupon.trim().toUpperCase() !== (preview.coupon_code ?? "") ||
      payment !== preview.payment_method);
  const previewExpired = !!preview && preview.expires_at_ms <= now;
  const walletBalance = wallet.data ? `${money(wallet.data.balance_usd)} USD` : "—";
  function purchaseStatus(plan: Plan) {
    const current = subscriptions.data?.items.find((item) => item.provider_id === plan.provider_id);
    const paid =
      !!current &&
      current.plan_type !== "free" &&
      !current.expired &&
      (current.expires_at === null || Date.parse(current.expires_at) > now);
    const same = paid && current.plan_id === plan.id;
    const lower =
      paid &&
      !same &&
      current.current_price_cents != null &&
      current.current_duration_days != null &&
      plan.sale_price_usd !== null &&
      Number(plan.sale_price_usd) * 100 * current.current_duration_days <=
        current.current_price_cents * plan.duration_days;
    const unavailable = !!current && (!current.enabled || (paid && current.expires_at === null));
    return {
      disabled:
        !plans.ready ||
        !subscriptions.ready ||
        plan.sale_price_usd === null ||
        unavailable ||
        lower ||
        busy,
      label: unavailable
        ? "请联系管理员"
        : lower
          ? "到期后可更换"
          : same
            ? "续订"
            : paid
              ? "升级套餐"
              : "下单",
    };
  }
  function beginCheckout(plan: Plan) {
    setViewedId(undefined);
    setSelected(plan);
    setPreview(undefined);
    setOrder(undefined);
    setCoupon("");
    setPayment("wallet");
    void actions.run("preview", () => loadPreview(plan, "", "wallet"), { success: "" });
  }
  async function loadPreview(
    selectedPlan: Plan,
    code: string,
    method: string,
    preserveDraft = false,
  ) {
    const plan = plans.data?.items.find((item) => item.id === selectedPlan.id);
    if (!plan) throw new Error("该套餐已停止购买，请刷新套餐列表");
    const current = subscriptions.data?.items.find((item) => item.provider_id === plan.provider_id);
    const value = await request<CheckoutPreview>("/checkout/preview", {
      method: "POST",
      body: {
        plan_id: plan.id,
        plan_revision: plan.revision,
        subscription_revision: current?.revision ?? null,
        payment_method: method,
        coupon_code: code.trim(),
      },
    });
    setSelected(plan);
    setPreview(value);
    if (!preserveDraft) setCoupon(value.coupon_code ?? "");
  }
  async function pay() {
    if (!order) return;
    try {
      setOrder(
        await request<Order>(`/orders/${encodeURIComponent(order.id)}/pay`, { method: "POST" }),
      );
    } catch (error) {
      const latest = await request<Order>(`/orders/${encodeURIComponent(order.id)}`).catch(
        () => undefined,
      );
      if (latest) setOrder(latest);
      throw error;
    } finally {
      refresh();
    }
  }
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" onClick={refresh}>
          刷新
        </Button>
        <Button variant="outline" asChild>
          <Link href="/orders/">我的订单</Link>
        </Button>
        <Badge variant="secondary">
          钱包 {wallet.data ? money(wallet.data.balance_usd) : "—"} USD
        </Badge>
      </div>
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        {plans.data?.items.map((plan) => {
          const purchase = purchaseStatus(plan);
          return (
            <Card
              key={plan.id}
              className="cursor-pointer"
              onClick={(event) => {
                if (!(event.target as HTMLElement).closest("a, button")) setViewedId(plan.id);
              }}
            >
              <CardHeader>
                <CardTitle>
                  <Button
                    variant="link"
                    className="h-auto p-0 text-base text-foreground"
                    onClick={() => setViewedId(plan.id)}
                  >
                    {plan.name}
                  </Button>
                </CardTitle>
                <CardDescription>
                  {plan.provider_id === "chatgpt"
                    ? "ChatGPT"
                    : plan.provider_id === "grok"
                      ? "Grok"
                      : plan.provider_id}{" "}
                  · {plan.duration_days} 天 ·{" "}
                  {plan.sale_price_usd === null ? "暂未定价" : money(plan.sale_price_usd)}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <div
                  className="prose prose-sm prose-neutral dark:prose-invert line-clamp-3 max-h-16 max-w-none break-words [&_*]:my-0 [&_h1]:text-sm [&_h2]:text-sm [&_h3]:text-sm [&_pre]:whitespace-pre-wrap"
                  aria-label="套餐描述摘要"
                >
                  <Markdown remarkPlugins={[remarkGfm]} skipHtml components={{ img: () => null }}>
                    {plan.description}
                  </Markdown>
                </div>
                <div className="flex items-center gap-2">
                  <Button variant="outline" onClick={() => setViewedId(plan.id)}>
                    查看详情
                  </Button>
                  <Button disabled={purchase.disabled} onClick={() => beginCheckout(plan)}>
                    {purchase.label}
                  </Button>
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>
      {plans.data?.items.length === 0 && (
        <p className="text-sm text-muted-foreground">暂无可购买的套餐</p>
      )}
      <Dialog
        open={!!viewedId}
        onOpenChange={(open) => {
          if (!open) setViewedId(undefined);
        }}
      >
        <DialogContent className="flex max-h-[90dvh] flex-col sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{viewed?.name ?? "套餐详情"}</DialogTitle>
            <DialogDescription>套餐介绍、订阅权益与可用模型</DialogDescription>
          </DialogHeader>
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <div className="space-y-5 pr-3">
              <dl className="grid grid-cols-[auto_1fr_auto_1fr] gap-x-4 gap-y-2 text-sm">
                <dt className="text-muted-foreground">平台</dt>
                <dd>
                  {viewed?.provider_id === "chatgpt"
                    ? "ChatGPT"
                    : viewed?.provider_id === "grok"
                      ? "Grok"
                      : (viewed?.provider_id ?? "—")}
                </dd>
                <dt className="text-muted-foreground">售价</dt>
                <dd>
                  {viewed?.sale_price_usd == null
                    ? "暂未定价"
                    : `${money(viewed.sale_price_usd)} USD`}
                </dd>
                <dt className="text-muted-foreground">有效期</dt>
                <dd>{viewed ? `${viewed.duration_days} 天` : "—"}</dd>
                <dt className="text-muted-foreground">可用模型</dt>
                <dd>{viewed?.models.length ?? "—"}</dd>
              </dl>
              <section className="space-y-2" aria-label="套餐描述">
                <h3 className="text-sm font-medium">套餐描述</h3>
                <div className="prose prose-sm prose-neutral dark:prose-invert max-w-none break-words [&_table]:block [&_table]:overflow-x-auto">
                  <Markdown remarkPlugins={[remarkGfm]} skipHtml>
                    {viewed?.description}
                  </Markdown>
                </div>
                {viewed && !viewed.description?.trim() && (
                  <p className="text-sm text-muted-foreground">暂无描述</p>
                )}
              </section>
              <section className="space-y-2" aria-label="额度规则">
                <h3 className="text-sm font-medium">额度规则</h3>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>周期</TableHead>
                      <TableHead>额度（USD）</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {viewed?.spending_windows.map((window) => (
                      <TableRow key={window.duration_seconds}>
                        <TableCell>
                          {window.duration_seconds >= 86400
                            ? `${window.duration_seconds / 86400} 天`
                            : `${window.duration_seconds / 3600} 小时`}
                        </TableCell>
                        <TableCell>
                          {window.cost_limit_usd === null ? "不限额" : money(window.cost_limit_usd)}
                        </TableCell>
                      </TableRow>
                    ))}
                    {viewed?.spending_windows.length === 0 && (
                      <TableRow>
                        <TableCell colSpan={2}>不限额</TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
                {(viewed?.spending_windows.length ?? 0) > 1 && (
                  <p className="text-xs text-muted-foreground">
                    各周期额度同时生效，短周期用量也计入长周期。
                  </p>
                )}
              </section>
              <section className="space-y-2" aria-label="可用模型">
                <h3 className="text-sm font-medium">可用模型</h3>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>模型</TableHead>
                      <TableHead>类型</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {viewed?.models.map((model) => (
                      <TableRow key={`${model.provider_id}/${model.model}`}>
                        <TableCell className="break-all whitespace-normal">{model.model}</TableCell>
                        <TableCell>
                          {model.kind === "text"
                            ? "文本"
                            : model.kind === "image"
                              ? "图像"
                              : model.kind}
                        </TableCell>
                      </TableRow>
                    ))}
                    {viewed?.models.length === 0 && (
                      <TableRow>
                        <TableCell colSpan={2}>暂无可用模型</TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              </section>
            </div>
          </ScrollArea>
          <DialogFooter>
            <Button variant="outline" onClick={() => setViewedId(undefined)}>
              关闭
            </Button>
            <Button
              disabled={!viewed || purchaseStatus(viewed).disabled}
              onClick={() => {
                if (viewed) beginCheckout(viewed);
              }}
            >
              {viewed ? purchaseStatus(viewed).label : "下单"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
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
                          busy ||
                          !coupon.trim() ||
                          !selected ||
                          !plans.ready ||
                          !subscriptions.ready
                        }
                        onClick={() => {
                          if (selected && coupon.trim())
                            void actions.run(
                              "apply-coupon",
                              () => loadPreview(selected, coupon, payment),
                              { success: "优惠码已使用" },
                            );
                        }}
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
                          onClick={() => {
                            if (selected)
                              void actions.run(
                                "remove-coupon",
                                () => loadPreview(selected, "", payment),
                                { success: "优惠码已移除" },
                              );
                          }}
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
                onClick={() => {
                  if (selected)
                    void actions.run(
                      "preview",
                      () => loadPreview(selected, preview?.coupon_code ?? "", payment, true),
                      { success: "预览已更新" },
                    );
                }}
              >
                {actions.isBusy("preview") && <Spinner />}刷新预览
              </Button>
            )}
            {stage === 1 && (
              <Button
                disabled={
                  busy ||
                  !preview ||
                  dirty ||
                  previewExpired ||
                  !plans.ready ||
                  !subscriptions.ready
                }
                onClick={() =>
                  void actions.run(
                    "confirm-order",
                    async () => {
                      if (!preview) return;
                      try {
                        setOrder(
                          await request<Order>("/orders", {
                            method: "POST",
                            body: { preview_token: preview.preview_token },
                          }),
                        );
                      } finally {
                        refresh();
                      }
                    },
                    { success: "订单已创建，请确认支付" },
                  )
                }
              >
                {actions.isBusy("confirm-order") && <Spinner />}确认订单
              </Button>
            )}
            {status === "pending" && (
              <>
                <Button
                  variant="outline"
                  disabled={busy}
                  onClick={() =>
                    void actions.run(
                      "cancel-order",
                      async () => {
                        if (order)
                          setOrder(
                            await request<Order>(`/orders/${encodeURIComponent(order.id)}/cancel`, {
                              method: "POST",
                            }),
                          );
                      },
                      { success: "订单已取消" },
                    )
                  }
                >
                  取消订单
                </Button>
                <Button
                  disabled={busy || !wallet.ready}
                  onClick={() =>
                    void actions.run("pay-order", pay, { success: "支付成功，订阅已更新" })
                  }
                >
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
    </>
  );
}
