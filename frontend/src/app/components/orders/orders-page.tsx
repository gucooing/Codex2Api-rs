"use client";
import { OrderDetailDialog } from "@/app/components/orders/order-detail-dialog";
import { emptyFilters, useOrdersPage } from "@/app/data/orders";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import { userOptionLabel, type UserOption } from "@/lib/user-lookup";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
import { cents, orderKinds, orderStatus, orderStatuses, orderTime } from "@/lib/orders";

export default function OrdersPage() {
  const {
    filters,
    setFilters,
    setApplied,
    plans,
    selectedUser,
    setSelectedUser,
    userLookup,
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
    columns,
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
          <Field className="w-48">
            <FieldLabel className="sr-only" htmlFor={`${id}-filter-user`}>
              用户
            </FieldLabel>
            <Combobox<UserOption>
              items={userLookup.data?.items ?? []}
              value={selectedUser}
              onValueChange={(item) => {
                setSelectedUser(item);
                setFilters((v) => ({ ...v, user_id: item?.id ?? "" }));
                userLookup.setSearch("");
              }}
              itemToStringLabel={userOptionLabel}
              itemToStringValue={(item) => item.id}
              isItemEqualToValue={(item, value) => item.id === value.id}
              filter={null}
              open={userLookup.open}
              onOpenChange={(open, details) => {
                userLookup.setOpen(open);
                if (open && details.reason !== "input-change") userLookup.setSearch("");
              }}
              onInputValueChange={(text, details) => {
                if (details.reason === "input-change") {
                  userLookup.setSearch(text);
                  if (!text) {
                    setSelectedUser(null);
                    setFilters((v) => ({ ...v, user_id: "" }));
                  }
                }
              }}
            >
              <ComboboxInput
                id={`${id}-filter-user`}
                placeholder="搜索选择用户"
                showClear
                className="w-full"
                maxLength={128}
              />
              <ComboboxContent>
                <ComboboxEmpty>
                  {userLookup.loading ? "加载中…" : userLookup.error ? "加载失败" : "没有匹配用户"}
                </ComboboxEmpty>
                <ComboboxList aria-busy={userLookup.loading}>
                  {(item: UserOption) => (
                    <ComboboxItem key={item.id} value={item}>
                      {userOptionLabel(item)}
                    </ComboboxItem>
                  )}
                </ComboboxList>
              </ComboboxContent>
            </Combobox>
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
              setSelectedUser(null);
              userLookup.setSearch("");
            }}
          >
            重置
          </Button>
        </form>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline">显示列</Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel>{columns.mobile ? "手机显示列" : "桌面显示列"}</DropdownMenuLabel>
            {columns.labels.map((label) => (
              <DropdownMenuCheckboxItem
                key={label}
                checked={columns.isVisible(label)}
                disabled={columns.count === 1 && columns.isVisible(label)}
                onSelect={(event) => event.preventDefault()}
                onCheckedChange={(checked) => columns.setVisible(label, checked === true)}
              >
                {label}
              </DropdownMenuCheckboxItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={columns.showAll}>显示全部列</DropdownMenuItem>
            <DropdownMenuItem onSelect={columns.reset}>恢复默认列</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead hidden={!columns.isVisible("用户")}>用户</TableHead>
                <TableHead hidden={!columns.isVisible("订单 / 套餐")}>订单 / 套餐</TableHead>
                <TableHead hidden={!columns.isVisible("应付")}>应付（USD）</TableHead>
                <TableHead hidden={!columns.isVisible("状态")}>状态</TableHead>
                <TableHead hidden={!columns.isVisible("创建时间")}>创建时间</TableHead>
                <TableHead hidden={!columns.isVisible("操作")}>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {resource.data?.items.map((order) => (
                <TableRow key={order.id}>
                  <TableCell hidden={!columns.isVisible("用户")}>{order.username}</TableCell>
                  <TableCell hidden={!columns.isVisible("订单 / 套餐")}>
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
                  <TableCell hidden={!columns.isVisible("应付")}>
                    {cents(order.amount_cents)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("状态")}>
                    <Badge variant="secondary">{orderStatuses[orderStatus(order, now)]}</Badge>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("创建时间")}>
                    {orderTime(order.created_at_ms)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作")}>
                    <Button variant="outline" onClick={() => setSelected(order)}>
                      详情
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {resource.data?.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={columns.count}>暂无订单</TableCell>
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
