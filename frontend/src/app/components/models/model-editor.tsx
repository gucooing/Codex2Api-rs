"use client";
import { useModelEditor } from "@/app/data/models";
import { Button } from "@/components/ui/button";
import { CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { type Model, type ModelPreset } from "@/lib/api";
import { emptyBasePrice, priceFields } from "@/lib/model-pricing";
import { Plus, Trash2, X } from "lucide-react";

export function ModelEditor({
  model,
  presets,
  onClose,
  onSaved,
}: {
  model: Model;
  presets: ModelPreset[] | undefined;
  onClose: () => void;
  onSaved: () => void;
}) {
  const {
    dialogFocus,
    fieldId,
    actions,
    value,
    pricing,
    presetVersion,
    matchedPreset,
    applyPreset,
    update,
    changePricing,
    updateRange,
    handleSubmit,
  } = useModelEditor({ model, presets, onSaved });
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !actions.running.size) onClose();
      }}
    >
      <DialogContent
        {...dialogFocus}
        showCloseButton={false}
        className="flex max-h-[90dvh] min-h-0 flex-col sm:max-w-3xl"
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (actions.running.size) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (actions.running.size) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{model.revision === null ? "添加模型" : "编辑模型"}</DialogTitle>
          <DialogDescription>{"价格仅应用于保存后的新请求。"}</DialogDescription>
        </DialogHeader>
        <DialogClose asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="absolute right-4 top-4"
            aria-label="关闭"
            disabled={actions.running.size > 0}
          >
            <X />
          </Button>
        </DialogClose>
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          aria-busy={actions.isBusy("app\\models\\page.tsx:form:6")}
          onSubmit={(event) => handleSubmit(event)}
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={actions.isBusy("app\\models\\page.tsx:form:6")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      模型信息
                    </CardTitle>
                  </div>
                  <div className="grid sm:grid-cols-3 gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={fieldId + "-field-7" + "-" + encodeURIComponent(String("提供商"))}
                      >
                        {"提供商"}
                      </FieldLabel>
                      <Select
                        value={value.provider_id}
                        onValueChange={(next) =>
                          ((provider) => update("provider_id", provider))(
                            next ===
                              fieldId +
                                "-field-7" +
                                "-" +
                                encodeURIComponent(String("提供商")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                        disabled={model.revision !== null}
                      >
                        <SelectTrigger
                          id={fieldId + "-field-7" + "-" + encodeURIComponent(String("提供商"))}
                          aria-label={"提供商"}
                          data-empty={String(value.provider_id) === "" ? "true" : undefined}
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              [
                                { value: "chatgpt", label: "ChatGPT" },
                                { value: "grok", label: "Grok" },
                              ].find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {[
                            { value: "chatgpt", label: "ChatGPT" },
                            { value: "grok", label: "Grok" },
                          ].map((option) => (
                            <SelectItem
                              key={option.value}
                              value={
                                option.value ||
                                fieldId +
                                  "-field-7" +
                                  "-" +
                                  encodeURIComponent(String("提供商")) +
                                  "-empty"
                              }
                              disabled={"disabled" in option && Boolean(option.disabled)}
                            >
                              {option.label}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </Field>
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId + "-field-8" + "-" + encodeURIComponent(String("模型名称"))
                        }
                      >
                        {"模型名称"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-8" + "-" + encodeURIComponent(String("模型名称"))}
                        aria-label={"模型名称"}
                        required
                        maxLength={256}
                        readOnly={model.revision !== null}
                        disabled={model.revision === null && !presets}
                        value={value.model}
                        onChange={(e) => update("model", e.target.value)}
                      />
                    </Field>
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId + "-field-9" + "-" + encodeURIComponent(String("计费方式"))
                        }
                      >
                        {"计费方式"}
                      </FieldLabel>
                      <Select
                        value={value.kind}
                        onValueChange={(next) =>
                          ((kind) => update("kind", kind as Model["kind"]))(
                            next ===
                              fieldId +
                                "-field-9" +
                                "-" +
                                encodeURIComponent(String("计费方式")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                      >
                        <SelectTrigger
                          id={fieldId + "-field-9" + "-" + encodeURIComponent(String("计费方式"))}
                          aria-label={"计费方式"}
                          data-empty={String(value.kind) === "" ? "true" : undefined}
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              [
                                { value: "text", label: "文本 · Token 计费" },
                                { value: "image", label: "图像 · 按张与分辨率计费" },
                              ].find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {[
                            { value: "text", label: "文本 · Token 计费" },
                            { value: "image", label: "图像 · 按张与分辨率计费" },
                          ].map((option) => (
                            <SelectItem
                              key={option.value}
                              value={
                                option.value ||
                                fieldId +
                                  "-field-9" +
                                  "-" +
                                  encodeURIComponent(String("计费方式")) +
                                  "-empty"
                              }
                              disabled={"disabled" in option && Boolean(option.disabled)}
                            >
                              {option.label}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </Field>
                  </div>
                  <Field orientation="horizontal">
                    <Switch
                      id={fieldId + "-field-10" + "-" + encodeURIComponent(String("启用模型"))}
                      checked={value.enabled}
                      onCheckedChange={(enabled) => update("enabled", enabled)}
                    />
                    <div>
                      <FieldLabel
                        htmlFor={
                          fieldId + "-field-10" + "-" + encodeURIComponent(String("启用模型"))
                        }
                      >
                        {"启用模型"}
                      </FieldLabel>
                    </div>
                  </Field>
                </section>
                <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={!matchedPreset?.token_prices.length}
                    onClick={() => matchedPreset && applyPreset(value, matchedPreset)}
                  >
                    应用预设价格
                  </Button>
                  {matchedPreset?.source_url ? (
                    <span>
                      {presetVersion ? "已应用预设 · " : "可用预设 · "}
                      <a
                        href={matchedPreset.source_url}
                        target="_blank"
                        rel="noreferrer"
                        className="underline underline-offset-4"
                      >
                        官方参考价
                      </a>{" "}
                      · {matchedPreset.verified_at}
                    </span>
                  ) : (
                    <span>{!presets ? "正在加载价格预设" : "暂无完整预设，请填写自定义价格"}</span>
                  )}
                </div>
                {value.kind === "text" ? (
                  <FieldSet className="gap-4">
                    <FieldLegend>基础价格（美元 / 百万 Token）</FieldLegend>
                    {pricing.ranges.map((row, index) => (
                      <section className="space-y-3" key={index}>
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <strong>
                            {index === 0 ? "基础区间" : `上下文区间 ${index}`} ·{" "}
                            {row.min_input_tokens.toLocaleString()} —{" "}
                            {pricing.ranges[index + 1]
                              ? (pricing.ranges[index + 1].min_input_tokens - 1).toLocaleString()
                              : "不限"}{" "}
                            Token
                          </strong>
                          {index > 0 && (
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              onClick={() =>
                                changePricing({
                                  ...pricing,
                                  ranges: pricing.ranges.filter((_, i) => i !== index),
                                })
                              }
                            >
                              <Trash2 />
                              移除区间
                            </Button>
                          )}
                        </div>
                        {index > 0 && (
                          <Field className="max-w-64">
                            <FieldLabel htmlFor={`${fieldId}-range-${index}`}>
                              上下文起点（Token）
                            </FieldLabel>
                            <Input
                              id={`${fieldId}-range-${index}`}
                              type="number"
                              min={1}
                              max={10000000}
                              step={1}
                              required
                              value={row.min_input_tokens}
                              onChange={(e) =>
                                updateRange(index, { min_input_tokens: Number(e.target.value) })
                              }
                            />
                          </Field>
                        )}
                        <Field>
                          <FieldLabel htmlFor={`${fieldId}-range-${index}-end`}>
                            输入 Token 上限（留空不限）
                          </FieldLabel>
                          <Input
                            id={`${fieldId}-range-${index}-end`}
                            type="number"
                            min={row.min_input_tokens}
                            max={10000000}
                            step={1}
                            value={row.max_input_tokens ?? ""}
                            onChange={(e) =>
                              updateRange(index, {
                                max_input_tokens:
                                  e.target.value === "" ? null : Number(e.target.value),
                              })
                            }
                          />
                        </Field>
                        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                          {priceFields.map(([key, label]) => (
                            <Field key={key}>
                              <FieldLabel htmlFor={`${fieldId}-range-${index}-${key}`}>
                                {label}
                              </FieldLabel>
                              <Input
                                id={`${fieldId}-range-${index}-${key}`}
                                type="number"
                                min={0}
                                step="0.000001"
                                required
                                value={row[key]}
                                onChange={(e) => updateRange(index, { [key]: e.target.value })}
                              />
                            </Field>
                          ))}
                        </div>
                      </section>
                    ))}
                    <Button
                      type="button"
                      variant="outline"
                      className="w-fit"
                      onClick={() =>
                        changePricing({
                          ...pricing,
                          ranges: [
                            ...pricing.ranges,
                            emptyBasePrice(
                              pricing.ranges.length
                                ? pricing.ranges.at(-1)!.min_input_tokens + 1
                                : 0,
                            ),
                          ],
                        })
                      }
                    >
                      <Plus />
                      {pricing.ranges.length ? "添加上下文区间" : "添加基础价格"}
                    </Button>
                    <FieldSet className="gap-3">
                      <FieldLegend>服务档位倍率</FieldLegend>
                      {(["fast", "flex"] as const).map((tier) => (
                        <section className="space-y-3" key={tier}>
                          <div className="grid gap-3 sm:grid-cols-2">
                            <Field>
                              <FieldLabel htmlFor={`${fieldId}-${tier}-mode`}>
                                {tier === "fast" ? "Fast" : "Flex"}
                              </FieldLabel>
                              <Select
                                value={pricing[tier].mode}
                                onValueChange={(mode) =>
                                  changePricing({ ...pricing, [tier]: { ...pricing[tier], mode } })
                                }
                                disabled={actions.isBusy("app\\models\\page.tsx:form:6")}
                              >
                                <SelectTrigger id={`${fieldId}-${tier}-mode`}>
                                  <SelectValue />
                                </SelectTrigger>
                                <SelectContent position="popper">
                                  <SelectItem value="off">未定价</SelectItem>
                                  <SelectItem value="multiplier">按倍率计价</SelectItem>
                                  {pricing[tier].rows.length > 0 && (
                                    <SelectItem value="custom">原有自定义价格</SelectItem>
                                  )}
                                </SelectContent>
                              </Select>
                            </Field>
                            {pricing[tier].mode === "multiplier" && (
                              <Field>
                                <FieldLabel htmlFor={`${fieldId}-${tier}-multiplier`}>
                                  {tier === "fast" ? "Fast" : "Flex"} 倍率
                                </FieldLabel>
                                <Input
                                  id={`${fieldId}-${tier}-multiplier`}
                                  type="number"
                                  min={0}
                                  step="0.000001"
                                  required
                                  value={pricing[tier].multiplier}
                                  onChange={(e) =>
                                    changePricing({
                                      ...pricing,
                                      [tier]: { ...pricing[tier], multiplier: e.target.value },
                                    })
                                  }
                                />
                              </Field>
                            )}
                          </div>
                          {pricing[tier].mode === "custom" &&
                            pricing[tier].rows.map((row, index) => (
                              <div key={index} className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
                                <Field>
                                  <FieldLabel htmlFor={`${fieldId}-${tier}-${index}-start`}>
                                    上下文起点
                                  </FieldLabel>
                                  <Input
                                    id={`${fieldId}-${tier}-${index}-start`}
                                    type="number"
                                    min={0}
                                    max={10000000}
                                    step={1}
                                    required
                                    value={row.min_input_tokens}
                                    onChange={(e) =>
                                      changePricing({
                                        ...pricing,
                                        [tier]: {
                                          ...pricing[tier],
                                          rows: pricing[tier].rows.map((old, i) =>
                                            i === index
                                              ? { ...old, min_input_tokens: Number(e.target.value) }
                                              : old,
                                          ),
                                        },
                                      })
                                    }
                                  />
                                </Field>
                                <Field>
                                  <FieldLabel htmlFor={`${fieldId}-${tier}-${index}-end`}>
                                    输入 Token 上限（留空不限）
                                  </FieldLabel>
                                  <Input
                                    id={`${fieldId}-${tier}-${index}-end`}
                                    type="number"
                                    min={row.min_input_tokens}
                                    max={10000000}
                                    step={1}
                                    value={row.max_input_tokens ?? ""}
                                    onChange={(e) =>
                                      changePricing({
                                        ...pricing,
                                        [tier]: {
                                          ...pricing[tier],
                                          rows: pricing[tier].rows.map((old, i) =>
                                            i === index
                                              ? {
                                                  ...old,
                                                  max_input_tokens:
                                                    e.target.value === ""
                                                      ? null
                                                      : Number(e.target.value),
                                                }
                                              : old,
                                          ),
                                        },
                                      })
                                    }
                                  />
                                </Field>
                                {priceFields.map(([key, label]) => (
                                  <Field key={key}>
                                    <FieldLabel htmlFor={`${fieldId}-${tier}-${index}-${key}`}>
                                      {label}
                                    </FieldLabel>
                                    <Input
                                      id={`${fieldId}-${tier}-${index}-${key}`}
                                      type="number"
                                      min={0}
                                      step="0.000001"
                                      required
                                      value={row[key]}
                                      onChange={(e) =>
                                        changePricing({
                                          ...pricing,
                                          [tier]: {
                                            ...pricing[tier],
                                            rows: pricing[tier].rows.map((old, i) =>
                                              i === index ? { ...old, [key]: e.target.value } : old,
                                            ),
                                          },
                                        })
                                      }
                                    />
                                  </Field>
                                ))}
                              </div>
                            ))}
                        </section>
                      ))}
                    </FieldSet>
                  </FieldSet>
                ) : (
                  <FieldSet className="space-y-3">
                    <FieldLegend>图像价格（美元 / 张）</FieldLegend>
                    <div className="space-y-4">
                      {value.image_prices.map((row, index) => (
                        <div className="flex flex-wrap items-end gap-2" key={index}>
                          <Field>
                            <FieldLabel
                              htmlFor={
                                fieldId +
                                "-field-16" +
                                "-" +
                                String(index) +
                                "-" +
                                encodeURIComponent(String("分辨率档位"))
                              }
                            >
                              {"分辨率档位"}
                            </FieldLabel>
                            <Select
                              value={row.resolution}
                              onValueChange={(next) =>
                                ((resolution) =>
                                  update(
                                    "image_prices",
                                    value.image_prices.map((item, i) =>
                                      i === index ? { ...item, resolution } : item,
                                    ),
                                  ))(
                                  next ===
                                    fieldId +
                                      "-field-16" +
                                      "-" +
                                      String(index) +
                                      "-" +
                                      encodeURIComponent(String("分辨率档位")) +
                                      "-empty"
                                    ? ""
                                    : next,
                                )
                              }
                              required={true}
                            >
                              <SelectTrigger
                                id={
                                  fieldId +
                                  "-field-16" +
                                  "-" +
                                  String(index) +
                                  "-" +
                                  encodeURIComponent(String("分辨率档位"))
                                }
                                aria-label={"分辨率档位"}
                                data-required="true"
                                data-empty={String(row.resolution) === "" ? "true" : undefined}
                                className="w-full"
                              >
                                <SelectValue
                                  placeholder={
                                    [
                                      { value: "", label: "请选择" },
                                      ...[
                                        ["0.5K", 512],
                                        ["1K", 1024],
                                        ["2K", 2560],
                                        ["4K", 4096],
                                        ["8K", 8192],
                                      ].map(([tier, max]) => ({
                                        value: String(tier),
                                        label: `${tier} · 长边 ≤ ${max}px`,
                                        disabled: value.image_prices.some(
                                          (item, i) => i !== index && item.resolution === tier,
                                        ),
                                      })),
                                    ].find((option) => option.value === "")?.label ?? "请选择"
                                  }
                                />
                              </SelectTrigger>
                              <SelectContent position="popper">
                                {[
                                  { value: "", label: "请选择" },
                                  ...[
                                    ["0.5K", 512],
                                    ["1K", 1024],
                                    ["2K", 2560],
                                    ["4K", 4096],
                                    ["8K", 8192],
                                  ].map(([tier, max]) => ({
                                    value: String(tier),
                                    label: `${tier} · 长边 ≤ ${max}px`,
                                    disabled: value.image_prices.some(
                                      (item, i) => i !== index && item.resolution === tier,
                                    ),
                                  })),
                                ].map((option) => (
                                  <SelectItem
                                    key={option.value}
                                    value={
                                      option.value ||
                                      fieldId +
                                        "-field-16" +
                                        "-" +
                                        String(index) +
                                        "-" +
                                        encodeURIComponent(String("分辨率档位")) +
                                        "-empty"
                                    }
                                    disabled={"disabled" in option && Boolean(option.disabled)}
                                  >
                                    {option.label}
                                  </SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                          </Field>
                          <Field>
                            <FieldLabel
                              htmlFor={
                                fieldId +
                                "-field-17" +
                                "-" +
                                String(index) +
                                "-" +
                                encodeURIComponent(String("每张价格"))
                              }
                            >
                              {"每张价格"}
                            </FieldLabel>
                            <Input
                              id={
                                fieldId +
                                "-field-17" +
                                "-" +
                                String(index) +
                                "-" +
                                encodeURIComponent(String("每张价格"))
                              }
                              aria-label={"每张价格"}
                              required
                              type="number"
                              min="0"
                              step="0.000000001"
                              value={row.price}
                              onChange={(e) =>
                                update(
                                  "image_prices",
                                  value.image_prices.map((item, i) =>
                                    i === index ? { ...item, price: e.target.value } : item,
                                  ),
                                )
                              }
                            />
                          </Field>
                          <Button
                            type="button"
                            variant="outline"
                            onClick={() =>
                              update(
                                "image_prices",
                                value.image_prices.filter((_, i) => i !== index),
                              )
                            }
                          >
                            移除
                          </Button>
                        </div>
                      ))}
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() =>
                          update("image_prices", [
                            ...value.image_prices,
                            { resolution: "", price: "" },
                          ])
                        }
                      >
                        <Plus />
                        添加分辨率档位
                      </Button>
                    </div>
                  </FieldSet>
                )}
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
            {onClose && (
              <Button
                type="button"
                variant="outline"
                disabled={actions.isBusy("app\\models\\page.tsx:form:6")}
                onClick={onClose}
              >
                {"取消"}
              </Button>
            )}
            <Button
              type="submit"
              disabled={
                actions.isBusy("app\\models\\page.tsx:form:6") ||
                (model.revision === null && !presets)
              }
            >
              {actions.isBusy("app\\models\\page.tsx:form:6") && <Spinner />}
              {actions.isBusy("app\\models\\page.tsx:form:6") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </DialogContent>
    </Dialog>
  );
}
