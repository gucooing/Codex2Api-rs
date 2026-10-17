"use client";
import { CouponEditorDialog } from "@/app/components/coupons/coupon-editor-dialog";
import { localDate, useCouponsPage } from "@/app/data/coupons";
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
import { cents, orderTime } from "@/lib/orders";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Plus,
  RotateCcw,
  Search,
} from "lucide-react";

export default function CouponsPage() {
  const {
    plans,
    edit,
    setEdit,
    search,
    setSearch,
    setApplied,
    actions,
    id,
    resource,
    pagination,
    update,
    busy,
  } = useCouponsPage();
  return (
    <>
      <Card size="sm">
        <CardContent>
          <form
            noValidate
            className="flex flex-wrap items-center gap-1.5 sm:gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied(search.trim());
              resource.reload(1);
            }}
          >
            <Field className="min-w-0 flex-1 sm:w-52 sm:flex-none">
              <FieldLabel className="sr-only" htmlFor={`${id}-search`}>
                优惠码 / 名称
              </FieldLabel>
              <Input
                id={`${id}-search`}
                placeholder="搜索优惠码或名称"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </Field>
            <Button type="submit" size="sm">
              <Search />
              查询 / 刷新
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                setSearch("");
                setApplied("");
                resource.reload(1);
              }}
            >
              <RotateCcw />
              重置
            </Button>
            <Button
              type="button"
              size="sm"
              className="ml-auto"
              disabled={!resource.ready || !plans.ready}
              onClick={() =>
                setEdit({
                  code: "",
                  name: "",
                  enabled: true,
                  discount: "",
                  minimum: "0",
                  plan: "all",
                  starts: localDate(Date.now()),
                  ends: localDate(Date.now() + 30 * 86400000),
                  max: "",
                  perUser: "1",
                  revision: null,
                })
              }
            >
              <Plus />
              创建优惠券
            </Button>
          </form>
        </CardContent>
      </Card>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>优惠码 / 名称</TableHead>
                <TableHead>优惠 / 门槛</TableHead>
                <TableHead>适用套餐</TableHead>
                <TableHead>有效期</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>已用 / 预留 / 上限</TableHead>
                <TableHead>每人上限</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {pagination.rows.map((c) => (
                <TableRow key={c.id}>
                  <TableCell>
                    <span className="font-mono font-medium">{c.code}</span>
                    <p className="text-xs text-muted-foreground">{c.name}</p>
                  </TableCell>
                  <TableCell className="tabular-nums">
                    {cents(c.discount_cents)} / {cents(c.minimum_cents)}
                  </TableCell>
                  <TableCell>
                    {c.plan_id
                      ? (plans.data?.items.find((p) => p.id === c.plan_id)?.name ?? "—")
                      : "全部付费套餐"}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground tabular-nums">
                    {orderTime(c.starts_at_ms)}
                    <br />
                    {orderTime(c.ends_at_ms)}
                  </TableCell>
                  <TableCell>
                    <Badge variant={c.enabled ? "secondary" : "outline"}>
                      {c.enabled ? "启用" : "停用"}
                    </Badge>
                  </TableCell>
                  <TableCell className="tabular-nums">
                    {c.used_count} / {c.reserved_count} / {c.max_uses ?? "不限"}
                  </TableCell>
                  <TableCell className="tabular-nums">{c.per_user_limit}</TableCell>
                  <TableCell>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!resource.ready || !plans.ready}
                      onClick={() =>
                        setEdit({
                          id: c.id,
                          code: c.code,
                          name: c.name,
                          enabled: c.enabled,
                          discount: (c.discount_cents / 100).toFixed(2),
                          minimum: (c.minimum_cents / 100).toFixed(2),
                          plan: c.plan_id ?? "all",
                          starts: localDate(c.starts_at_ms),
                          ends: localDate(c.ends_at_ms),
                          max: c.max_uses?.toString() ?? "",
                          perUser: String(c.per_user_limit),
                          revision: c.revision,
                        })
                      }
                    >
                      编辑
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {resource.data && pagination.rows.length === 0 && (
                <TableRow>
                  <TableCell colSpan={8} className="text-center text-muted-foreground">
                    暂无优惠券
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="优惠券分页">
            <PaginationContent className="flex-wrap justify-end gap-1">
              <PaginationItem>
                <Select {...pagination.size}>
                  <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper" side="bottom" align="end">
                    {[10, 20, 30, 50].map((n) => (
                      <SelectItem key={n} value={String(n)}>
                        {n} 条/页
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </PaginationItem>
              <PaginationItem className="mr-2 text-xs text-muted-foreground">
                共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
              </PaginationItem>
              <PaginationItem>
                <Button
                  type="button"
                  variant="outline"
                  size="icon-sm"
                  aria-label="首页"
                  {...pagination.first}
                >
                  <ChevronsLeft />
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button
                  type="button"
                  variant="outline"
                  size="icon-sm"
                  aria-label="上一页"
                  {...pagination.previous}
                >
                  <ChevronLeft />
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Input className="h-7 w-14 text-center tabular-nums" {...pagination.input} />
              </PaginationItem>
              <PaginationItem>
                <Button
                  type="button"
                  variant="outline"
                  size="icon-sm"
                  aria-label="下一页"
                  {...pagination.next}
                >
                  <ChevronRight />
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button
                  type="button"
                  variant="outline"
                  size="icon-sm"
                  aria-label="末页"
                  {...pagination.last}
                >
                  <ChevronsRight />
                </Button>
              </PaginationItem>
            </PaginationContent>
          </Pagination>
        </CardContent>
      </Card>
      <CouponEditorDialog
        edit={edit}
        busy={busy}
        setEdit={setEdit}
        actions={actions}
        resource={resource}
        plans={plans}
        id={id}
        update={update}
      />
    </>
  );
}
