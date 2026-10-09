"use client";
import { usePlanEditor } from "@/app/data/plans";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { type Plan, type SpendingWindow } from "@/lib/api";
import { modelKey } from "@/lib/domain";
import { subscriptionLabel } from "@/lib/subscriptions";
import { X } from "lucide-react";

export function PlanEditor({
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
  const {
    value,
    setValue,
    search,
    setSearch,
    models,
    tags,
    focus,
    id,
    busy,
    tiers,
    update,
    outer,
    inner,
    windowChange,
    choices,
    handleSubmit,
  } = usePlanEditor({ plan, ready, onSaved });
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
        <form noValidate className="flex min-h-0 flex-col gap-4" onSubmit={(e) => handleSubmit(e)}>
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
                        {tags.data?.items.map((t) => (
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
