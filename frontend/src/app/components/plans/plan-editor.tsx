"use client";
import { planProviders, usePlanEditor } from "@/app/data/plans";
import { ModelRulesTable } from "@/components/forms/model-rules-table";
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
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { type Plan, type SpendingWindow } from "@/lib/api";
import { providerLabel } from "@/lib/providers";
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
    rpmInput,
    concurrencyInput,
    value,
    selectProvider,
    tiers,
    search,
    setSearch,
    models,
    tags,
    rules,
    toggleTag,
    toggleModel,
    selectVisible,
    setRule,
    focus,
    id,
    busy,
    update,
    outer,
    inner,
    windowChange,
    handleSubmit,
  } = usePlanEditor({ plan, ready, onSaved });
  const disabled = !ready || !models.ready || !tags.ready || busy;
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
        className="flex max-h-[90dvh] flex-col sm:max-w-4xl"
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
          <Tabs defaultValue="plan" className="min-h-0">
            <TabsList>
              <TabsTrigger value="plan">套餐设置</TabsTrigger>
              <TabsTrigger value="models">模型与规则</TabsTrigger>
            </TabsList>
            <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-15rem)]">
              <TabsContent value="plan" className="pr-3">
                <FieldSet disabled={disabled}>
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
                        <FieldLabel htmlFor={`${id}-provider`}>所属平台</FieldLabel>
                        <Select
                          value={value.provider_id}
                          disabled={!!plan.id}
                          onValueChange={selectProvider}
                        >
                          <SelectTrigger id={`${id}-provider`} className="w-full">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent position="popper">
                            {planProviders.map((provider) => (
                              <SelectItem value={provider.value} key={provider.value}>
                                {provider.label}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </Field>
                      <Field>
                        <FieldLabel htmlFor={`${id}-tier`}>官方客户端订阅档位</FieldLabel>
                        <Select
                          value={value.plan_type}
                          onValueChange={(v) => update("plan_type", v)}
                        >
                          <SelectTrigger id={`${id}-tier`} className="w-full">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent position="popper">
                            {!tiers.some((tier) => tier.value === value.plan_type) && (
                              <SelectItem value={value.plan_type}>
                                原有档位：{subscriptionLabel(value.plan_type, value.provider_id)}
                              </SelectItem>
                            )}
                            {tiers.map((tier) => (
                              <SelectItem value={tier.value} key={tier.value}>
                                {tier.label}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </Field>
                      <Field>
                        <FieldLabel htmlFor={`${id}-rpm`}>RPM 限制</FieldLabel>
                        <Input
                          id={`${id}-rpm`}
                          type="number"
                          min={0}
                          max={1000000}
                          step={1}
                          {...rpmInput}
                        />
                      </Field>
                      <Field>
                        <FieldLabel htmlFor={`${id}-concurrency`}>并发限制</FieldLabel>
                        <Input
                          id={`${id}-concurrency`}
                          type="number"
                          min={0}
                          max={1000000}
                          step={1}
                          {...concurrencyInput}
                        />
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
                          value={value.sale_price_usd ?? ""}
                          onChange={(e) => update("sale_price_usd", e.target.value || null)}
                          placeholder="留空不开放购买"
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
                          value={value.duration_days}
                          onChange={(e) => update("duration_days", Number(e.target.value))}
                        />
                      </Field>
                      <Field orientation="horizontal">
                        <Switch
                          id={`${id}-allow-purchase`}
                          checked={value.allow_purchase}
                          onCheckedChange={(v) => update("allow_purchase", v)}
                        />
                        <FieldLabel htmlFor={`${id}-allow-purchase`}>允许购买</FieldLabel>
                      </Field>
                    </div>
                    <FieldSet>
                      <FieldLegend variant="label">供应号池标签</FieldLegend>
                      <div className="grid gap-2 sm:grid-cols-3">
                        {tags.data?.items.map((tag) => (
                          <Field key={tag.id} orientation="horizontal">
                            <Checkbox
                              id={`${id}-tag-${tag.id}`}
                              checked={value.tag_ids.includes(tag.id)}
                              onCheckedChange={(checked) => toggleTag(tag.id, checked === true)}
                            />
                            <FieldLabel htmlFor={`${id}-tag-${tag.id}`}>
                              {tag.name} · {providerLabel(tag.provider_id)}
                            </FieldLabel>
                          </Field>
                        ))}
                      </div>
                      {tags.ready && tags.data?.items.length === 0 && (
                        <FieldDescription>暂无标签。</FieldDescription>
                      )}
                    </FieldSet>
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
                              v
                                ? [outer, { duration_seconds: 18000, cost_limit_usd: null }]
                                : [outer],
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
              </TabsContent>
              <TabsContent value="models" className="pr-3">
                <ModelRulesTable
                  id={id}
                  options={models.data?.items}
                  ready={models.ready}
                  rules={rules}
                  baseMultiplier="1"
                  search={search}
                  onSearch={setSearch}
                  onToggle={toggleModel}
                  onSelectVisible={selectVisible}
                  onRule={setRule}
                  disabled={disabled}
                />
              </TabsContent>
            </ScrollArea>
          </Tabs>
          <DialogFooter>
            <Button type="button" variant="outline" disabled={busy} onClick={onClose}>
              取消
            </Button>
            <Button type="submit" disabled={disabled}>
              保存
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
