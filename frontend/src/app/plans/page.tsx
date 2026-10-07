"use client";
import { useColumnVisibility } from "@/lib/columns";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuCheckboxItem,
  DropdownMenuSeparator,
  DropdownMenuItem,
} from "@/components/ui/dropdown-menu";

import { useId, useState } from "react";
import { X } from "lucide-react";
import {
  request,
  type Plan,
  type Plans,
  type Model,
  type SupplierTag,
  type List,
  type SpendingWindow,
} from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import { useTablePagination } from "@/lib/pagination";
import { planWrite, modelKey } from "@/lib/domain";
import {
  subscriptionChoices,
  grokSubscriptionChoices,
  subscriptionLabel,
  subscriptionValue,
} from "@/lib/subscriptions";
import { money } from "@/lib/format";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Field, FieldLabel, FieldSet, FieldGroup } from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Table,
  TableHeader,
  TableHead,
  TableBody,
  TableRow,
  TableCell,
} from "@/components/ui/table";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogClose,
} from "@/components/ui/dialog";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";

const emptyPlan: Plan = {
  description: "",
  id: "",
  provider_id: "chatgpt",
  name: "",
  plan_type: "plus",
  model_access: "all",
  models: [],
  sale_price_usd: null,
  duration_days: 30,
  supplier_tag_id: null,
  allow_purchase: true,
  revision: 0,
  updated_at_ms: 0,
  spending_windows: [
    { duration_seconds: 604800, cost_limit_usd: null },
    { duration_seconds: 18000, cost_limit_usd: null },
  ],
};
const windowLabel = (duration: number) =>
  duration === 18000 ? "5 小时" : duration === 604800 ? "7 天" : "30 天";
export default function PlansPage() {
  const columns = useColumnVisibility(
    "plans",
    ["套餐", "售价 / 有效期", "模型权限", "费用上限", "允许购买", "操作"],
    ["套餐", "售价 / 有效期", "操作"],
  );
  const resource = useResource<Plans>("/plans");
  const [search, setSearch] = useState("");
  const [applied, setApplied] = useState("");
  const [editing, setEditing] = useState<Plan>();
  const actions = useActions();
  const id = useId();
  const rows =
    resource.data?.items.filter((p) =>
      `${p.name} ${p.provider_id} ${subscriptionLabel(p.plan_type, p.provider_id)}`
        .toLowerCase()
        .includes(applied.toLowerCase()),
    ) ?? [];
  const pagination = useTablePagination(rows, applied, !!resource.data);
  useErrorToast(resource.error);
  return (
    <>
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              setApplied(search.trim());
              resource.reload();
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
                resource.reload();
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
                        onClick={() =>
                          void actions.run(
                            `delete-${plan.id}`,
                            async () => {
                              await request(`/plans/${plan.id}`, {
                                method: "DELETE",
                                body: { revision: plan.revision },
                              });
                              resource.reload();
                            },
                            { confirm: "删除此套餐？仍被使用的套餐无法删除。", danger: true },
                          )
                        }
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
function PlanEditor({
  plan,
  ready,
  onClose,
  onSaved,
}: {
  plan: Plan;
  ready: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [value, setValue] = useState(() => ({
    ...plan,
    description: plan.description ?? "",
    plan_type: subscriptionValue(plan.plan_type, plan.provider_id),
  }));
  const [search, setSearch] = useState("");
  const models = useResource<List<Model>>("/models");
  const tags = useResource<List<SupplierTag>>("/supplier-tags");
  const actions = useActions();
  const focus = useDialogFocus();
  const id = useId();
  const busy = actions.isBusy("plan-save");
  const tiers = value.provider_id === "grok" ? grokSubscriptionChoices : subscriptionChoices;
  const update = <K extends keyof Plan>(key: K, next: Plan[K]) =>
    setValue((v) => ({ ...v, [key]: next }));
  const outer = value.spending_windows[0];
  const inner = value.spending_windows[1];
  const windowChange = (index: number, window: SpendingWindow) =>
    update(
      "spending_windows",
      value.spending_windows.map((w, i) => (i === index ? window : w)),
    );
  const choices =
    models.data?.items.filter(
      (m) =>
        m.provider_id === value.provider_id &&
        (m.enabled || value.models.some((v) => modelKey(v) === modelKey(m))) &&
        m.model.toLowerCase().includes(search.toLowerCase()),
    ) ?? [];
  useErrorToast(models.error);
  useErrorToast(tags.error);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent
        {...focus}
        aria-describedby={undefined}
        showCloseButton={false}
        className="flex max-h-[90dvh] flex-col sm:max-w-2xl"
        onEscapeKeyDown={(event) => {
          if (busy) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (busy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{plan.id ? "编辑套餐" : "添加套餐"}</DialogTitle>
        </DialogHeader>
        <DialogClose asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="absolute top-4 right-4"
            aria-label="关闭"
            disabled={busy}
          >
            <X />
          </Button>
        </DialogClose>
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          onSubmit={(e) =>
            actions.submit(e, "plan-save", async () => {
              if (!ready || !models.ready || !tags.ready)
                throw new Error("请先加载套餐、模型和号池资料");
              if (value.model_access === "selected" && !value.models.length)
                throw new Error("请至少选择一个模型");
              await request(plan.id ? `/plans/${plan.id}` : "/plans", {
                method: plan.id ? "PUT" : "POST",
                body: planWrite(value),
              });
              onSaved();
            })
          }
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-13rem)]">
            <FieldSet disabled={!ready || !models.ready || !tags.ready || busy}>
              <FieldGroup className="gap-4">
                <div className="grid gap-3 sm:grid-cols-2">
                  <Field>
                    <FieldLabel htmlFor={`${id}-name`}>套餐名称</FieldLabel>
                    <Input
                      id={`${id}-name`}
                      required
                      maxLength={128}
                      value={value.name}
                      onChange={(e) => update("name", e.target.value)}
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-provider`}>提供商</FieldLabel>
                    <Select
                      value={value.provider_id}
                      disabled={!!plan.id}
                      onValueChange={(v) =>
                        setValue((old) => ({
                          ...old,
                          provider_id: v,
                          plan_type: "free",
                          models: [],
                          supplier_tag_id: null,
                          allow_purchase: false,
                        }))
                      }
                    >
                      <SelectTrigger id={`${id}-provider`}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        <SelectItem value="chatgpt">ChatGPT</SelectItem>
                        <SelectItem value="grok">Grok</SelectItem>
                      </SelectContent>
                    </Select>
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-tier`}>官方客户端订阅档位</FieldLabel>
                    <Select
                      value={value.plan_type}
                      disabled={plan.plan_type === "free"}
                      onValueChange={(v) =>
                        setValue((old) => ({
                          ...old,
                          plan_type: v,
                          allow_purchase: v === "free" ? false : old.allow_purchase,
                        }))
                      }
                    >
                      <SelectTrigger id={`${id}-tier`}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        {!tiers.some((tier) => tier.value === value.plan_type) && (
                          <SelectItem value={value.plan_type}>
                            原有档位：{subscriptionLabel(value.plan_type, value.provider_id)}
                          </SelectItem>
                        )}
                        {tiers.map((v) => (
                          <SelectItem value={v.value} key={v.value}>
                            {v.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </Field>
                  <Field className="sm:col-span-2">
                    <FieldLabel htmlFor={`${id}-description`}>套餐描述</FieldLabel>
                    <Textarea
                      id={`${id}-description`}
                      value={value.description}
                      maxLength={20000}
                      rows={5}
                      onChange={(event) => update("description", event.target.value)}
                      placeholder="介绍套餐的特点、适用场景和权益"
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-price`}>售价（USD）</FieldLabel>
                    <Input
                      id={`${id}-price`}
                      inputMode="decimal"
                      disabled={value.plan_type === "free"}
                      value={value.sale_price_usd ?? ""}
                      onChange={(e) => update("sale_price_usd", e.target.value || null)}
                      placeholder={value.plan_type === "free" ? "自动提供" : "留空不开放购买"}
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-duration`}>购买有效时长（天）</FieldLabel>
                    <Input
                      id={`${id}-duration`}
                      type="number"
                      required
                      min={1}
                      max={3650}
                      disabled={value.plan_type === "free"}
                      value={value.duration_days}
                      onChange={(e) => update("duration_days", Number(e.target.value))}
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-pool`}>用户订阅默认供应号池</FieldLabel>
                    <Select
                      value={value.supplier_tag_id ?? "none"}
                      onValueChange={(v) => update("supplier_tag_id", v === "none" ? null : v)}
                    >
                      <SelectTrigger id={`${id}-pool`}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        <SelectItem value="none">未设置</SelectItem>
                        {tags.data?.items
                          .filter((t) => t.provider_id === value.provider_id)
                          .map((t) => (
                            <SelectItem key={t.id} value={t.id}>
                              {t.name}
                            </SelectItem>
                          ))}
                      </SelectContent>
                    </Select>
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-model-access`}>模型权限</FieldLabel>
                    <Select
                      value={value.model_access}
                      onValueChange={(v) => update("model_access", v as Plan["model_access"])}
                    >
                      <SelectTrigger id={`${id}-model-access`}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        <SelectItem value="none">无模型</SelectItem>
                        <SelectItem value="all">全部已启用模型</SelectItem>
                        <SelectItem value="selected">指定模型</SelectItem>
                      </SelectContent>
                    </Select>
                  </Field>
                  <Field orientation="horizontal">
                    <Switch
                      id={`${id}-allow-purchase`}
                      checked={value.allow_purchase}
                      disabled={value.plan_type === "free"}
                      onCheckedChange={(v) => update("allow_purchase", v)}
                    />
                    <FieldLabel htmlFor={`${id}-allow-purchase`}>允许购买</FieldLabel>
                  </Field>
                </div>
                {value.model_access === "selected" && (
                  <FieldGroup className="gap-2">
                    <Field>
                      <FieldLabel htmlFor={`${id}-models`}>搜索模型</FieldLabel>
                      <Input
                        id={`${id}-models`}
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                      />
                    </Field>
                    <div className="flex flex-wrap gap-2">
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() =>
                          update(
                            "models",
                            Array.from(
                              new Map(
                                [...value.models, ...choices].map((m) => [
                                  modelKey(m),
                                  { provider_id: m.provider_id, model: m.model },
                                ]),
                              ).values(),
                            ),
                          )
                        }
                      >
                        选择当前结果
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() =>
                          update(
                            "models",
                            value.models.filter(
                              (m) => !choices.some((c) => modelKey(c) === modelKey(m)),
                            ),
                          )
                        }
                      >
                        清除当前结果
                      </Button>
                    </div>
                    {choices.map((m) => (
                      <Field orientation="horizontal" key={modelKey(m)}>
                        <Checkbox
                          id={`${id}-${m.model}`}
                          checked={value.models.some((v) => modelKey(v) === modelKey(m))}
                          onCheckedChange={(checked) =>
                            update(
                              "models",
                              checked
                                ? [...value.models, { provider_id: m.provider_id, model: m.model }]
                                : value.models.filter((v) => modelKey(v) !== modelKey(m)),
                            )
                          }
                        />
                        <FieldLabel htmlFor={`${id}-${m.model}`}>{m.model}</FieldLabel>
                      </Field>
                    ))}
                  </FieldGroup>
                )}
                <Field orientation="horizontal">
                  <Switch
                    id={`${id}-windows`}
                    checked={!!outer}
                    onCheckedChange={(enabled) =>
                      update(
                        "spending_windows",
                        enabled ? [{ duration_seconds: 604800, cost_limit_usd: "0" }] : [],
                      )
                    }
                  />
                  <FieldLabel htmlFor={`${id}-windows`}>设置费用窗口</FieldLabel>
                </Field>
                <div className="grid gap-3 sm:grid-cols-2">
                  <Field>
                    <FieldLabel htmlFor={`${id}-outer`}>外层额度周期</FieldLabel>
                    <Select
                      disabled={!outer}
                      value={String(outer?.duration_seconds ?? 604800)}
                      onValueChange={(v) =>
                        windowChange(0, {
                          ...outer,
                          duration_seconds: Number(v) as SpendingWindow["duration_seconds"],
                        })
                      }
                    >
                      <SelectTrigger id={`${id}-outer`}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        <SelectItem value="604800">7 天</SelectItem>
                        <SelectItem value="2592000">30 天</SelectItem>
                      </SelectContent>
                    </Select>
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-outer-limit`}>外层费用上限（USD）</FieldLabel>
                    <Input
                      id={`${id}-outer-limit`}
                      disabled={!outer}
                      inputMode="decimal"
                      value={outer?.cost_limit_usd ?? ""}
                      placeholder="留空不限额"
                      onChange={(e) =>
                        windowChange(0, { ...outer, cost_limit_usd: e.target.value || null })
                      }
                    />
                  </Field>
                  <Field orientation="horizontal">
                    <Switch
                      id={`${id}-inner`}
                      disabled={!outer}
                      checked={!!inner}
                      onCheckedChange={(v) =>
                        update(
                          "spending_windows",
                          v ? [outer, { duration_seconds: 18000, cost_limit_usd: null }] : [outer],
                        )
                      }
                    />
                    <FieldLabel htmlFor={`${id}-inner`}>启用 5 小时内层窗口</FieldLabel>
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-inner-limit`}>5 小时费用上限（USD）</FieldLabel>
                    <Input
                      id={`${id}-inner-limit`}
                      inputMode="decimal"
                      disabled={!inner}
                      value={inner?.cost_limit_usd ?? ""}
                      placeholder="留空不限额"
                      onChange={(e) => {
                        if (inner)
                          windowChange(1, { ...inner, cost_limit_usd: e.target.value || null });
                      }}
                    />
                  </Field>
                </div>
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <DialogFooter>
            <Button type="button" variant="outline" disabled={busy} onClick={onClose}>
              取消
            </Button>
            <Button type="submit" disabled={!ready || !models.ready || !tags.ready || busy}>
              保存
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
