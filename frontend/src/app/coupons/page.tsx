"use client";
import { useId, useState } from "react";
import { request, type Plan } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useActions, useErrorToast } from "@/lib/actions";
import { useTablePagination } from "@/lib/pagination";
import { cents, orderTime } from "@/lib/orders";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel } from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
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
import { ScrollArea } from "@/components/ui/scroll-area";

type Coupon = {
  id: string;
  code: string;
  name: string;
  enabled: boolean;
  discount_cents: number;
  minimum_cents: number;
  plan_id: string | null;
  starts_at_ms: number;
  ends_at_ms: number;
  max_uses: number | null;
  per_user_limit: number;
  revision: number;
  used_count: number;
  reserved_count: number;
};
type Edit = {
  id?: string;
  code: string;
  name: string;
  enabled: boolean;
  discount: string;
  minimum: string;
  plan: string;
  starts: string;
  ends: string;
  max: string;
  perUser: string;
  revision: number | null;
};
const localDate = (at: number) => {
  const d = new Date(at);
  return new Date(at - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
};
function amount(text: string) {
  if (!/^\d+(\.\d{1,2})?$/.test(text.trim()))
    throw new Error("金额须为非负 USD 金额，最多两位小数");
  const [whole, fraction = ""] = text.trim().split(".");
  const result = Number(whole) * 100 + Number(fraction.padEnd(2, "0"));
  if (!Number.isSafeInteger(result) || result > 100000000) throw new Error("金额超出范围");
  return result;
}
export default function CouponsPage() {
  const resource = useResource<{ items: Coupon[] }>("/coupons");
  const plans = useResource<{ items: Plan[] }>("/plans");
  const [edit, setEdit] = useState<Edit>();
  const [search, setSearch] = useState("");
  const [applied, setApplied] = useState("");
  const actions = useActions();
  const id = useId();
  const pagination = useTablePagination(
    resource.data?.items.filter((c) =>
      `${c.code} ${c.name}`.toLowerCase().includes(applied.toLowerCase()),
    ) ?? [],
    "coupons",
    !!resource.data,
  );
  const update = (patch: Partial<Edit>) =>
    setEdit((value) => (value ? { ...value, ...patch } : value));
  const busy = actions.isBusy("save-coupon");
  useErrorToast(resource.error);
  useErrorToast(plans.error);
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
              resource.reload();
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
      <Dialog
        open={!!edit}
        onOpenChange={(open) => {
          if (!open && !busy) setEdit(undefined);
        }}
      >
        <DialogContent className="flex max-h-[90dvh] flex-col sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>{edit?.id ? "编辑优惠券" : "创建优惠券"}</DialogTitle>
            <DialogDescription>
              固定金额优惠券，按原订阅抵扣后的应付金额计算门槛。已确认订单保留当时的优惠，修改适用于之后的订单。
            </DialogDescription>
          </DialogHeader>
          <form
            noValidate
            className="flex min-h-0 flex-col gap-4"
            onSubmit={(event) =>
              actions.submit(
                event,
                "save-coupon",
                async () => {
                  if (!edit || !resource.ready || !plans.ready)
                    throw new Error("请先加载优惠券和套餐");
                  const starts = Date.parse(edit.starts),
                    ends = Date.parse(edit.ends);
                  const max = edit.max.trim() === "" ? null : Number(edit.max),
                    perUser = Number(edit.perUser);
                  if (!Number.isFinite(starts) || !Number.isFinite(ends) || ends <= starts)
                    throw new Error("请选择有效的起止时间");
                  if (
                    (max !== null && (!Number.isSafeInteger(max) || max < 1)) ||
                    !Number.isSafeInteger(perUser) ||
                    perUser < 1
                  )
                    throw new Error("使用次数须为正整数");
                  await request(edit.id ? `/coupons/${encodeURIComponent(edit.id)}` : "/coupons", {
                    method: edit.id ? "PUT" : "POST",
                    body: {
                      code: edit.code,
                      name: edit.name,
                      enabled: edit.enabled,
                      discount_cents: amount(edit.discount),
                      minimum_cents: amount(edit.minimum),
                      plan_id: edit.plan === "all" ? null : edit.plan,
                      starts_at_ms: starts,
                      ends_at_ms: ends,
                      max_uses: max,
                      per_user_limit: perUser,
                      revision: edit.revision,
                    },
                  });
                  setEdit(undefined);
                  resource.reload();
                },
                "优惠券已保存",
              )
            }
          >
            <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-15rem)]">
              <div className="grid gap-4 pr-3 sm:grid-cols-2">
                <Field>
                  <FieldLabel htmlFor={`${id}-code`}>优惠码</FieldLabel>
                  <Input
                    id={`${id}-code`}
                    value={edit?.code ?? ""}
                    onChange={(e) => update({ code: e.target.value })}
                    required
                    maxLength={64}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-name`}>名称</FieldLabel>
                  <Input
                    id={`${id}-name`}
                    value={edit?.name ?? ""}
                    onChange={(e) => update({ name: e.target.value })}
                    required
                    maxLength={120}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-discount`}>优惠金额（USD）</FieldLabel>
                  <Input
                    id={`${id}-discount`}
                    inputMode="decimal"
                    value={edit?.discount ?? ""}
                    onChange={(e) => update({ discount: e.target.value })}
                    required
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-minimum`}>最低应付金额（USD）</FieldLabel>
                  <Input
                    id={`${id}-minimum`}
                    inputMode="decimal"
                    value={edit?.minimum ?? ""}
                    onChange={(e) => update({ minimum: e.target.value })}
                    required
                  />
                </Field>
                <Field className="sm:col-span-2">
                  <FieldLabel htmlFor={`${id}-plan`}>适用套餐</FieldLabel>
                  <Select value={edit?.plan ?? "all"} onValueChange={(plan) => update({ plan })}>
                    <SelectTrigger id={`${id}-plan`}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="all">全部付费套餐</SelectItem>
                      {plans.data?.items
                        .filter((p) => p.plan_type !== "free")
                        .map((p) => (
                          <SelectItem key={p.id} value={p.id}>
                            {p.name}
                          </SelectItem>
                        ))}
                    </SelectContent>
                  </Select>
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-starts`}>开始时间</FieldLabel>
                  <Input
                    id={`${id}-starts`}
                    type="datetime-local"
                    value={edit?.starts ?? ""}
                    onChange={(e) => update({ starts: e.target.value })}
                    required
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-ends`}>结束时间</FieldLabel>
                  <Input
                    id={`${id}-ends`}
                    type="datetime-local"
                    value={edit?.ends ?? ""}
                    onChange={(e) => update({ ends: e.target.value })}
                    required
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-max`}>总使用次数（留空不限）</FieldLabel>
                  <Input
                    id={`${id}-max`}
                    inputMode="numeric"
                    value={edit?.max ?? ""}
                    onChange={(e) => update({ max: e.target.value })}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-per`}>每人使用次数</FieldLabel>
                  <Input
                    id={`${id}-per`}
                    inputMode="numeric"
                    value={edit?.perUser ?? ""}
                    onChange={(e) => update({ perUser: e.target.value })}
                    required
                  />
                </Field>
                <Field orientation="horizontal">
                  <FieldLabel htmlFor={`${id}-enabled`}>启用</FieldLabel>
                  <Switch
                    id={`${id}-enabled`}
                    checked={edit?.enabled ?? false}
                    onCheckedChange={(enabled) => update({ enabled })}
                  />
                </Field>
              </div>
            </ScrollArea>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={busy}
                onClick={() => setEdit(undefined)}
              >
                取消
              </Button>
              <Button disabled={busy || !resource.ready || !plans.ready}>保存</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}
