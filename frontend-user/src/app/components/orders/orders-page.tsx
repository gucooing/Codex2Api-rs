"use client";
import { OrderDetailDialog } from "@/app/components/orders/order-detail-dialog";
import { emptyFilters, useOrdersPage } from "@/app/data/orders";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
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
import { cents, orderKinds, orderStatus, orderStatuses } from "@/lib/orders";

export default function OrdersPage() {
  const {
    filters,
    setFilters,
    setApplied,
    plans,
    setPage,
    selected,
    setSelected,
    id,
    now,
    resource,
    pagination,
    actions,
    effective,
    busy,
    act,
  } = useOrdersPage();
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
      <OrderDetailDialog
        selected={selected}
        busy={busy}
        setSelected={setSelected}
        effective={effective}
        resource={resource}
        actions={actions}
        act={act}
      />
    </>
  );
}
