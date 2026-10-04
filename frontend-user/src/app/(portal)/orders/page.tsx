"use client";
import { useId, useState } from "react";
import { request } from "@/lib/api";
import { useResource } from "@/lib/resource";
import { useActions, useErrorToast } from "@/lib/actions";
import { usePageControls } from "@/lib/pagination";
import {
  cents,
  orderKinds,
  orderStatus,
  orderStatuses,
  orderTime,
  useOrderClock,
  type Order,
  type OrderPage,
} from "@/lib/orders";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";

const emptyFilters = { search: "", status: "all", plan_id: "all" };
export default function OrdersPage() {
  const [filters, setFilters] = useState(emptyFilters);
  const [applied, setApplied] = useState(emptyFilters);
  const plans = useResource<{ items: { id: string; name: string; provider_id: string }[] }>(
    "/orders/plans",
    0,
  );
  const [page, setPage] = useState(1);
  const [size, setSize] = useState(20);
  const [selected, setSelected] = useState<Order>();
  const id = useId();
  const now = useOrderClock();
  const resource = useResource<OrderPage>(
    `/orders?search=${encodeURIComponent(applied.search)}&status=${applied.status === "all" ? "" : applied.status}&plan_id=${applied.plan_id === "all" ? "" : encodeURIComponent(applied.plan_id)}&page=${page}&limit=${size}`,
    0,
  );
  const pagination = usePageControls(
    page,
    resource.data?.total,
    setPage,
    size,
    !resource.data,
    setSize,
  );
  const actions = useActions();
  const effective = selected ? orderStatus(selected, now) : undefined;
  const busy = actions.isBusy("order-action");

  useErrorToast(resource.error);
  useErrorToast(plans.error);
  async function act(action: "pay" | "cancel") {
    if (!selected) return;
    try {
      setSelected(
        await request<Order>(`/orders/${encodeURIComponent(selected.id)}/${action}`, {
          method: "POST",
        }),
      );
    } catch (error) {
      const latest = await request<Order>(`/orders/${encodeURIComponent(selected.id)}`).catch(
        () => undefined,
      );
      if (latest) setSelected(latest);
      throw error;
    } finally {
      resource.reload();
    }
  }
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <form
          aria-label="订单筛选"
          noValidate
          className="flex flex-wrap items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            setApplied({ ...filters, search: filters.search.trim() });
            setPage(1);
            resource.reload();
            plans.reload();
          }}
        >
          <Field className="w-40">
            <FieldLabel className="sr-only" htmlFor={`${id}-search`}>
              订单号
            </FieldLabel>
            <Input
              id={`${id}-search`}
              placeholder="订单号"
              maxLength={128}
              value={filters.search}
              onChange={(event) => setFilters((v) => ({ ...v, search: event.target.value }))}
            />
          </Field>
          <Field className="w-40">
            <FieldLabel className="sr-only" htmlFor={`${id}-filter-plan`}>
              套餐
            </FieldLabel>
            <Select
              value={filters.plan_id}
              onValueChange={(plan_id) => setFilters((v) => ({ ...v, plan_id }))}
            >
              <SelectTrigger id={`${id}-filter-plan`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="all">全部套餐</SelectItem>
                {plans.data?.items.map((plan) => (
                  <SelectItem key={plan.id} value={plan.id}>
                    {plan.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field className="w-32">
            <FieldLabel className="sr-only" htmlFor={`${id}-status`}>
              订单状态
            </FieldLabel>
            <Select
              value={filters.status}
              onValueChange={(status) => setFilters((v) => ({ ...v, status }))}
            >
              <SelectTrigger id={`${id}-status`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="all">全部状态</SelectItem>
                {Object.entries(orderStatuses).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Button type="submit">查询</Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              setFilters(emptyFilters);
              setApplied(emptyFilters);
              setPage(1);
              resource.reload();
            }}
          >
            重置
          </Button>
        </form>
      </div>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>订单 / 套餐</TableHead>
                <TableHead>应付（USD）</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {resource.data?.items.map((order) => (
                <TableRow key={order.id}>
                  <TableCell>
                    <div>
                      {order.plan_name} · {orderKinds[order.kind]}
                    </div>
                    <div
                      className="max-w-48 truncate text-xs text-muted-foreground"
                      title={order.id}
                    >
                      {order.id}
                    </div>
                  </TableCell>
                  <TableCell>{cents(order.amount_cents)}</TableCell>
                  <TableCell>
                    <Badge variant="secondary">{orderStatuses[orderStatus(order, now)]}</Badge>
                  </TableCell>
                  <TableCell>
                    <Button variant="outline" onClick={() => setSelected(order)}>
                      详情
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {resource.data?.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={4}>暂无订单</TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end">
            <PaginationContent className="flex-wrap">
              <PaginationItem>
                共 {resource.data?.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
              </PaginationItem>
              <PaginationItem>
                <Select {...pagination.size}>
                  <SelectTrigger aria-label="每页条数">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    {[10, 20, 30, 50].map((value) => (
                      <SelectItem key={value} value={String(value)}>
                        {value} 条
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...pagination.first}>
                  首页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...pagination.previous}>
                  上一页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Input className="w-16" {...pagination.input} />
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...pagination.next}>
                  下一页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...pagination.last}>
                  末页
                </Button>
              </PaginationItem>
            </PaginationContent>
          </Pagination>
        </CardContent>
      </Card>
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
                onClick={() =>
                  void actions.run("order-action", () => act("cancel"), { success: "订单已取消" })
                }
              >
                取消订单
              </Button>
            )}
            {effective === "pending" && (
              <Button
                disabled={busy || !resource.ready}
                onClick={() =>
                  void actions.run("order-action", () => act("pay"), {
                    success: "支付成功，订阅已更新",
                  })
                }
              >
                确认支付 {selected ? cents(selected.amount_cents) : ""}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
