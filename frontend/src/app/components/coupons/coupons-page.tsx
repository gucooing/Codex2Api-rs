"use client";
import { CouponEditorDialog } from "@/app/components/coupons/coupon-editor-dialog";
import { localDate, useCouponsPage } from "@/app/data/coupons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cents, orderTime } from "@/lib/orders";

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
      <Card>
        <CardContent>
          <form
            noValidate
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied(search.trim());
              resource.reload(1);
            }}
          >
            <Field className="w-60">
              <FieldLabel htmlFor={`${id}-search`}>优惠码 / 名称</FieldLabel>
              <Input
                id={`${id}-search`}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </Field>
            <Button type="submit" variant="outline">
              查询 / 刷新
            </Button>
            <Button
              type="button"
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
                    {c.code}
                    <p className="text-xs text-muted-foreground">{c.name}</p>
                  </TableCell>
                  <TableCell>
                    {cents(c.discount_cents)} / {cents(c.minimum_cents)}
                  </TableCell>
                  <TableCell>
                    {c.plan_id
                      ? (plans.data?.items.find((p) => p.id === c.plan_id)?.name ?? "—")
                      : "全部付费套餐"}
                  </TableCell>
                  <TableCell>
                    {orderTime(c.starts_at_ms)}
                    <br />
                    {orderTime(c.ends_at_ms)}
                  </TableCell>
                  <TableCell>
                    <Badge variant="secondary">{c.enabled ? "启用" : "停用"}</Badge>
                  </TableCell>
                  <TableCell>
                    {c.used_count} / {c.reserved_count} / {c.max_uses ?? "不限"}
                  </TableCell>
                  <TableCell>{c.per_user_limit}</TableCell>
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
                  <TableCell colSpan={8}>暂无优惠券</TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <div className="mt-3 flex flex-wrap items-center justify-end gap-2 text-sm">
            <span>共 {pagination.total ?? "—"} 条</span>
            <Button variant="outline" size="sm" {...pagination.previous}>
              上一页
            </Button>
            <Input className="w-16" {...pagination.input} />
            <span>/ {pagination.pages ?? "—"}</span>
            <Button variant="outline" size="sm" {...pagination.next}>
              下一页
            </Button>
          </div>
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
