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
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              setApplied(search.trim());
              resource.reload(1);
            }}
          >
            <Field className="w-56">
              <FieldLabel htmlFor={`${id}-search`}>搜索套餐</FieldLabel>
              <Input
                id={`${id}-search`}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </Field>
            <Button type="submit">查询</Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setSearch("");
                setApplied("");
                resource.reload(1);
              }}
            >
              重置
            </Button>
          </form>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" variant="outline">
                显示列
              </Button>
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
          <Button
            className="ml-auto"
            disabled={!resource.ready}
            onClick={() => setEditing({ ...emptyPlan })}
          >
            添加套餐
          </Button>
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
                  <TableCell hidden={!columns.isVisible("售价 / 有效期")}>
                    {plan.plan_type === "free"
                      ? "自动提供"
                      : plan.sale_price_usd === null
                        ? "未开放购买"
                        : money(plan.sale_price_usd)}
                    {plan.plan_type !== "free" && (
                      <p className="text-xs text-muted-foreground">{plan.duration_days} 天</p>
                    )}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("模型权限")}>
                    {plan.model_access === "all"
                      ? "全部已启用模型"
                      : plan.model_access === "none"
                        ? "无模型"
                        : `指定 ${plan.models.length} 个模型`}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("费用上限")}>
                    {plan.spending_windows.map((w) => (
                      <p key={w.duration_seconds}>
                        {windowLabel(w.duration_seconds)}：
                        {w.cost_limit_usd === null ? "不限额" : money(w.cost_limit_usd)}
                      </p>
                    ))}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("允许购买")}>
                    <Badge variant="secondary">
                      {plan.plan_type === "free"
                        ? "自动提供"
                        : plan.allow_purchase
                          ? "允许购买"
                          : "未开放购买"}
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
                        disabled={
                          !resource.ready ||
                          plan.plan_type === "free" ||
                          actions.isBusy(`delete-${plan.id}`)
                        }
                        onClick={() => handleClick(plan)}
                      >
                        删除
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="套餐分页">
            <PaginationContent className="flex-wrap">
              <PaginationItem>
                共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
              </PaginationItem>
              <PaginationItem>
                <Select {...pagination.size}>
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
