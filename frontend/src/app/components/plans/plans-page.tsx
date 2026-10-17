"use client";
import { PlanEditor } from "@/app/components/plans/plan-editor";
import { emptyPlan, usePlansPage, windowLabel } from "@/app/data/plans";
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
import { money } from "@/lib/format";
import { subscriptionLabel } from "@/lib/subscriptions";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  Plus,
  RotateCcw,
  Search,
} from "lucide-react";

export default function PlansPage() {
  const {
    columns,
    search,
    setSearch,
    setApplied,
    editing,
    setEditing,
    actions,
    id,
    resource,
    pagination,
    handleClick,
  } = usePlansPage();
  return (
    <>
      <Card size="sm">
        <CardContent className="flex flex-wrap items-center gap-2">
          <form
            className="flex w-full items-center gap-1.5 sm:w-auto sm:gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              setApplied(search.trim());
              resource.reload(1);
            }}
          >
            <Field className="min-w-0 flex-1 sm:w-52 sm:flex-none">
              <FieldLabel className="sr-only" htmlFor={`${id}-search`}>
                搜索套餐
              </FieldLabel>
              <Input
                id={`${id}-search`}
                placeholder="搜索套餐名称"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </Field>
            <Button type="submit" size="sm">
              <Search />
              查询
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
          </form>
          <div className="ml-auto flex flex-wrap items-center gap-1.5">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button type="button" variant="outline" size="sm">
                  <Columns3 />
                  显示列
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>
                  {columns.mobile ? "手机显示列" : "桌面显示列"}
                </DropdownMenuLabel>
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
            <Button
              size="sm"
              disabled={!resource.ready}
              onClick={() => setEditing({ ...emptyPlan })}
            >
              <Plus />
              添加套餐
            </Button>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead hidden={!columns.isVisible("套餐")}>套餐</TableHead>
                <TableHead hidden={!columns.isVisible("售价 / 有效期")}>售价 / 有效期</TableHead>
                <TableHead hidden={!columns.isVisible("模型权限")}>模型权限</TableHead>
                <TableHead hidden={!columns.isVisible("费用上限")}>费用上限</TableHead>
                <TableHead hidden={!columns.isVisible("允许购买")}>允许购买</TableHead>
                <TableHead hidden={!columns.isVisible("操作")}>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {pagination.rows.map((plan) => (
                <TableRow key={plan.id}>
                  <TableCell hidden={!columns.isVisible("套餐")}>
                    <span className="font-medium">{plan.name}</span>
                    <p className="text-xs text-muted-foreground">
                      {plan.provider_id} · {subscriptionLabel(plan.plan_type, plan.provider_id)}
                    </p>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("售价 / 有效期")} className="tabular-nums">
                    {plan.sale_price_usd === null ? "未开放购买" : money(plan.sale_price_usd)}
                    <p className="text-xs text-muted-foreground">{plan.duration_days} 天</p>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("模型权限")}>
                    {plan.model_access === "all"
                      ? "全部已启用模型"
                      : plan.model_access === "none"
                        ? "无模型"
                        : `指定 ${plan.models.length} 个模型`}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("费用上限")} className="tabular-nums">
                    {plan.spending_windows.map((w) => (
                      <p key={w.duration_seconds}>
                        {windowLabel(w.duration_seconds)}：
                        {w.cost_limit_usd === null ? "不限额" : money(w.cost_limit_usd)}
                      </p>
                    ))}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("允许购买")}>
                    <Badge variant={plan.allow_purchase ? "secondary" : "outline"}>
                      {plan.allow_purchase ? "允许购买" : "未开放购买"}
                    </Badge>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作")}>
                    <div className="flex gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={!resource.ready}
                        onClick={() => setEditing(plan)}
                      >
                        编辑
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={!resource.ready || actions.isBusy(`delete-${plan.id}`)}
                        onClick={() => handleClick(plan)}
                      >
                        删除
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {pagination.rows.length === 0 && (
                <TableRow>
                  <TableCell colSpan={columns.count} className="text-center text-muted-foreground">
                    暂无套餐
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="套餐分页">
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
      {editing && (
        <PlanEditor
          key={editing.id || "new"}
          plan={editing}
          ready={resource.ready}
          onClose={() => setEditing(undefined)}
          onSaved={() => {
            setEditing(undefined);
            resource.reload();
          }}
        />
      )}
    </>
  );
}
