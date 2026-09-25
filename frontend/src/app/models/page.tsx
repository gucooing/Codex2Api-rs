"use client";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useTablePagination } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight } from "lucide-react";

import { useDialogFocus } from "@/lib/actions";
import { ScrollArea } from "@/components/ui/scroll-area";

import { Card, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel, FieldSet, FieldGroup, FieldLegend } from "@/components/ui/field";
import { useId } from "react";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableBody,
  TableCell,
} from "@/components/ui/table";
import { Empty, EmptyDescription } from "@/components/ui/empty";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from "@/components/ui/dropdown-menu";
import { MoreHorizontal, X } from "lucide-react";
import { useActions, useErrorToast } from "@/lib/actions";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogClose,
} from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { money } from "@/lib/format";
import { useState } from "react";
import { Plus, Pencil, Search, RotateCcw, Trash2 } from "lucide-react";
import { request, type List, type Model, type TokenPrice } from "@/lib/api";
import { modelWrite } from "@/lib/domain";
import { useResource } from "@/lib/hooks";
import {
  pricingDraft,
  pricingRows,
  emptyBasePrice,
  priceFields,
  type PricingDraft,
  type BasePrice,
} from "@/lib/model-pricing";

const tokenRule = (): TokenPrice => ({
  tier: "standard",
  min_input_tokens: 0,
  input_rate: "",
  cached_rate: "",
  cache_write_rate: "",
  output_rate: "",
});
const emptyModel = (): Model => ({
  provider_id: "chatgpt",
  model: "",
  kind: "text",
  enabled: true,
  revision: null,
  token_prices: [tokenRule()],
  image_prices: [{ resolution: "", price: "" }],
});
export default function ModelsPage() {
  const fieldId = useId();
  const actions = useActions();
  const resource = useResource<List<Model>>("/models");
  const [editing, setEditing] = useState<Model>();
  const empty = { search: "", kind: "", status: "" };
  const [filters, setFilters] = useState(empty);
  const [applied, setApplied] = useState(empty);
  const items =
    resource.data?.items.filter(
      (model) =>
        `${model.model} ${model.provider_id}`
          .toLowerCase()
          .includes(applied.search.trim().toLowerCase()) &&
        (!applied.kind || model.kind === applied.kind) &&
        (!applied.status || model.enabled === (applied.status === "enabled")),
    ) ?? [];
  const pagination = useTablePagination(items, applied, resource.data !== undefined);
  useErrorToast(resource.error);
  return (
    <>
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters });
              resource.reload();
            }}
          >
            <Field className="w-44">
              <FieldLabel
                htmlFor={fieldId + "-field-1" + "-" + encodeURIComponent(String("搜索模型"))}
              >
                {"搜索模型"}
              </FieldLabel>
              <Input
                id={fieldId + "-field-1" + "-" + encodeURIComponent(String("搜索模型"))}
                aria-label={"搜索模型"}
                value={filters.search}
                onChange={(event) => setFilters({ ...filters, search: event.target.value })}
                placeholder="模型名称或提供商"
              />
            </Field>
            <Field className="w-44">
              <FieldLabel
                htmlFor={fieldId + "-field-2" + "-" + encodeURIComponent(String("模型类型"))}
              >
                {"模型类型"}
              </FieldLabel>
              <Select
                value={filters.kind}
                onValueChange={(next) =>
                  ((kind) => setFilters({ ...filters, kind }))(
                    next ===
                      fieldId + "-field-2" + "-" + encodeURIComponent(String("模型类型")) + "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-2" + "-" + encodeURIComponent(String("模型类型"))}
                  aria-label={"模型类型"}
                  data-required={false ? "true" : undefined}
                  data-empty={String(filters.kind) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部类型" },
                        { value: "text", label: "文本模型" },
                        { value: "image", label: "图像模型" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部类型" },
                    { value: "text", label: "文本模型" },
                    { value: "image", label: "图像模型" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-2" +
                          "-" +
                          encodeURIComponent(String("模型类型")) +
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
            <Field className="w-44">
              <FieldLabel
                htmlFor={fieldId + "-field-3" + "-" + encodeURIComponent(String("模型状态"))}
              >
                {"模型状态"}
              </FieldLabel>
              <Select
                value={filters.status}
                onValueChange={(next) =>
                  ((status) => setFilters({ ...filters, status }))(
                    next ===
                      fieldId + "-field-3" + "-" + encodeURIComponent(String("模型状态")) + "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-3" + "-" + encodeURIComponent(String("模型状态"))}
                  aria-label={"模型状态"}
                  data-required={false ? "true" : undefined}
                  data-empty={String(filters.status) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部状态" },
                        { value: "enabled", label: "已启用" },
                        { value: "disabled", label: "已停用" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部状态" },
                    { value: "enabled", label: "已启用" },
                    { value: "disabled", label: "已停用" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-3" +
                          "-" +
                          encodeURIComponent(String("模型状态")) +
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
            <div className="flex flex-wrap items-center gap-2 self-end">
              <Button type="submit">
                <Search />
                查询
              </Button>
              <Button
                type="button"
                variant="secondary"
                onClick={() => {
                  setFilters(empty);
                  setApplied(empty);
                  resource.reload();
                }}
              >
                <RotateCcw />
                重置
              </Button>
            </div>
          </form>
          <div className="flex flex-wrap items-center gap-2 self-end xl:ml-auto">
            {resource.error && (
              <Button type="button" variant="outline" size="sm" onClick={resource.reload}>
                重新加载
              </Button>
            )}
            {
              <Button type="button" onClick={() => setEditing(emptyModel())}>
                <Plus />
                添加模型
              </Button>
            }
          </div>
        </CardContent>
      </Card>

      {
        <Card>
          <CardContent className="space-y-4">
            <Table>
              <TableHeader>
                <TableRow>
                  {["模型", "计费方式", "价格规则", "状态", "操作"].map((label) => (
                    <TableHead key={label} scope="col">
                      {label}
                    </TableHead>
                  ))}
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.length ? (
                  <>
                    {pagination.rows.map((model) => (
                      <TableRow key={`${model.provider_id}/${model.model}`}>
                        <TableCell>
                          <strong>{model.model}</strong>
                          <CardDescription>{model.provider_id}</CardDescription>
                          {model.codex_metadata_status === "unavailable" && (
                            <CardDescription className="text-sm text-muted-foreground text-xs">
                              缺少已验证的 Codex 模型描述，暂不显示在 Codex 模型选择器。
                            </CardDescription>
                          )}
                        </TableCell>
                        <TableCell>
                          {model.kind === "text" ? "文本 · Token" : "图像 · 按张"}
                        </TableCell>
                        <TableCell>
                          {model.kind === "text" ? (
                            <>
                              <span>{model.token_prices.length} 条价格规则</span>
                              <CardDescription>
                                {[
                                  ...new Set(
                                    model.token_prices.map(
                                      (rule) =>
                                        ({ standard: "标准", fast: "快速", flex: "Flex" })[
                                          rule.tier
                                        ],
                                    ),
                                  ),
                                ].join(" · ") || "尚未配置价格"}
                              </CardDescription>
                              <CardDescription>美元 / 百万 Token</CardDescription>
                            </>
                          ) : (
                            model.image_prices.map((rule) => (
                              <div key={rule.resolution}>
                                {rule.resolution}：{money(rule.price)} / 张
                              </div>
                            ))
                          )}
                        </TableCell>
                        <TableCell>
                          <Badge variant={model.enabled ? "secondary" : "outline"}>
                            {model.enabled ? "已启用" : "已停用"}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-wrap items-center gap-2">
                            <Button variant="outline" onClick={() => setEditing(model)}>
                              <Pencil />
                              编辑
                            </Button>
                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                                <Button
                                  type="button"
                                  variant="ghost"
                                  size="icon-sm"
                                  aria-label="更多操作"
                                >
                                  <MoreHorizontal />
                                </Button>
                              </DropdownMenuTrigger>
                              <DropdownMenuContent align="end">
                                <DropdownMenuItem
                                  variant={false ? "destructive" : "default"}
                                  disabled={
                                    false || actions.isBusy("app\\models\\page.tsx:action:4")
                                  }
                                  onSelect={() =>
                                    void actions.run(
                                      "app\\models\\page.tsx:action:4",
                                      async () => {
                                        await request("/models/status", {
                                          method: "POST",
                                          body: {
                                            provider_id: model.provider_id,
                                            model: model.model,
                                            revision: model.revision,
                                            enabled: !model.enabled,
                                          },
                                        });
                                        resource.reload();
                                      },
                                      {
                                        confirm: model.enabled
                                          ? "停用此模型并停止接受新请求？"
                                          : undefined,
                                        danger: false,
                                        success: undefined,
                                      },
                                    )
                                  }
                                >
                                  {model.enabled ? "停用" : "启用"}
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  variant={true ? "destructive" : "default"}
                                  disabled={
                                    false || actions.isBusy("app\\models\\page.tsx:action:5")
                                  }
                                  onSelect={() =>
                                    void actions.run(
                                      "app\\models\\page.tsx:action:5",
                                      async () => {
                                        await request("/models/delete", {
                                          method: "POST",
                                          body: {
                                            provider_id: model.provider_id,
                                            model: model.model,
                                            revision: model.revision,
                                          },
                                        });
                                        resource.reload();
                                      },
                                      {
                                        confirm: "删除模型并停止接受新请求？历史用量和费用保留。",
                                        danger: true,
                                        success: undefined,
                                      },
                                    )
                                  }
                                >
                                  删除
                                </DropdownMenuItem>
                              </DropdownMenuContent>
                            </DropdownMenu>
                          </div>
                        </TableCell>
                      </TableRow>
                    ))}
                  </>
                ) : (
                  <TableRow>
                    <TableCell colSpan={["模型", "计费方式", "价格规则", "状态", "操作"].length}>
                      <Empty>
                        <EmptyDescription>
                          {resource.error ? "尚未取得模型数据" : "暂无符合条件的模型"}
                        </EmptyDescription>
                      </Empty>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
            <Pagination aria-label="记录分页" className="mt-3 justify-end">
              <PaginationContent className="flex-wrap justify-end gap-1">
                <PaginationItem>
                  <Select {...pagination.size}>
                    <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper" side="bottom" align="end">
                      {[10, 20, 30, 50].map((size) => (
                        <SelectItem key={size} value={String(size)}>
                          {size} 条/页
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
      }
      {editing && (
        <ModelEditor
          model={editing}
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
function ModelEditor({
  model,
  onClose,
  onSaved,
}: {
  model: Model;
  onClose: () => void;
  onSaved: () => void;
}) {
  const dialogFocus = useDialogFocus();

  const fieldId = useId();
  const actions = useActions();
  const [value, setValue] = useState(model);
  const update = <K extends keyof Model>(key: K, next: Model[K]) =>
    setValue((current) => ({ ...current, [key]: next }));
  const [pricing, setPricing] = useState(() => pricingDraft(model.token_prices));
  const [pricingChanged, setPricingChanged] = useState(false);
  const changePricing = (next: PricingDraft) => {
    setPricing(next);
    setPricingChanged(true);
  };
  const updateRange = (index: number, patch: Partial<BasePrice>) =>
    changePricing({
      ...pricing,
      ranges: pricing.ranges.map((row, i) => (i === index ? { ...row, ...patch } : row)),
    });
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
          onSubmit={(event) =>
            actions.submit(
              event,
              "app\\models\\page.tsx:form:6",
              async () => {
                await request("/models", {
                  method: "POST",
                  body: modelWrite({
                    ...value,
                    token_prices:
                      value.kind === "text" && pricingChanged
                        ? pricingRows(pricing)
                        : value.token_prices,
                  }),
                });
                onSaved();
              },
              "已保存",
            )
          }
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
                          data-required={false ? "true" : undefined}
                          data-empty={String(value.provider_id) === "" ? "true" : undefined}
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              [{ value: "chatgpt", label: "ChatGPT" }].find(
                                (option) => option.value === "",
                              )?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {[{ value: "chatgpt", label: "ChatGPT" }].map((option) => (
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
                          data-required={false ? "true" : undefined}
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
                                <SelectContent>
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
                                data-required={true ? "true" : undefined}
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
                      <CardDescription className="text-sm text-muted-foreground">
                        实际成功张数按长边档位计费，未配置档位保持未计价。
                      </CardDescription>
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
            <Button type="submit" disabled={actions.isBusy("app\\models\\page.tsx:form:6")}>
              {actions.isBusy("app\\models\\page.tsx:form:6") && <Spinner />}
              {actions.isBusy("app\\models\\page.tsx:form:6") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </DialogContent>
    </Dialog>
  );
}
