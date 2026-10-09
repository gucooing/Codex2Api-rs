"use client";
import { CheckoutDialog } from "@/app/components/plans/checkout-dialog";
import { PlanDetailDialog } from "@/app/components/plans/plan-detail-dialog";
import { usePlansPage } from "@/app/data/plans";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { money } from "@/lib/api";
import Link from "next/link";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

export default function PlansPage() {
  const {
    subscriptions,
    plans,
    planPage,
    wallet,
    actions,
    selected,
    setSelected,
    viewedId,
    setViewedId,
    viewed,
    preview,
    order,
    setOrder,
    coupon,
    setCoupon,
    payment,
    setPayment,
    id,
    refresh,
    status,
    stage,
    busy,
    detail,
    dirty,
    previewExpired,
    walletBalance,
    purchaseStatus,
    beginCheckout,
    loadPreview,
    pay,
  } = usePlansPage();
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
      <Pagination className="mt-3 justify-end">
        <PaginationContent className="flex-wrap">
          <PaginationItem>
            共 {planPage.total ?? "—"} 条 · {planPage.pages ?? "—"} 页
          </PaginationItem>
          <PaginationItem>
            <Select {...planPage.size}>
              <SelectTrigger aria-label="每页条数">
                <SelectValue />
              </SelectTrigger>
              <SelectContent side="bottom">
                {[10, 20, 30, 50].map((n) => (
                  <SelectItem key={n} value={String(n)}>
                    {n} 条
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </PaginationItem>
          <PaginationItem>
            <Button variant="outline" {...planPage.first}>
              首页
            </Button>
          </PaginationItem>
          <PaginationItem>
            <Button variant="outline" {...planPage.previous}>
              上一页
            </Button>
          </PaginationItem>
          <PaginationItem>
            <Input className="w-16" aria-label="页码" {...planPage.input} />
          </PaginationItem>
          <PaginationItem>
            <Button variant="outline" {...planPage.next}>
              下一页
            </Button>
          </PaginationItem>
          <PaginationItem>
            <Button variant="outline" {...planPage.last}>
              末页
            </Button>
          </PaginationItem>
        </PaginationContent>
      </Pagination>
      <PlanDetailDialog
        viewedId={viewedId}
        setViewedId={setViewedId}
        viewed={viewed}
        purchaseStatus={purchaseStatus}
        beginCheckout={beginCheckout}
      />
      <CheckoutDialog
        selected={selected}
        busy={busy}
        setSelected={setSelected}
        stage={stage}
        status={status}
        detail={detail}
        wallet={wallet}
        order={order}
        preview={preview}
        walletBalance={walletBalance}
        id={id}
        payment={payment}
        setPayment={setPayment}
        coupon={coupon}
        setCoupon={setCoupon}
        plans={plans}
        subscriptions={subscriptions}
        actions={actions}
        loadPreview={loadPreview}
        dirty={dirty}
        previewExpired={previewExpired}
        setOrder={setOrder}
        refresh={refresh}
        pay={pay}
      />
    </>
  );
}
