"use client";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useTablePagination } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight } from "lucide-react";
import { useDialogFocus } from "@/lib/actions";
import { ScrollArea } from "@/components/ui/scroll-area";

import { Card, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldLabel,
  FieldDescription,
  FieldSet,
  FieldGroup,
  FieldLegend,
} from "@/components/ui/field";
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
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from "@/components/ui/dropdown-menu";
import { MoreHorizontal, X, Inbox } from "lucide-react";
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
import { Checkbox } from "@/components/ui/checkbox";
import { money } from "@/lib/format";
import { useId, useState } from "react";
import { Plus, Search, RotateCcw, ChevronDown, Pencil } from "lucide-react";
import {
  request,
  type Plan,
  type Plans,
  type ModelRef,
  type Model,
  type List,
  type SpendingWindow,
} from "@/lib/api";
import { modelKey, sameProviderModels, planWrite } from "@/lib/domain";
import { useResource } from "@/lib/hooks";

import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Label } from "@/components/ui/label";

const emptyPlan: Plan = {
  id: "",
  name: "",
  provider_id: "chatgpt",
  model_access: "selected",
  models: [],
  free_model_access: "none",
  free_models: [],
  free_access_enabled: false,
  primary_cost_limit_usd: null,
  weekly_cost_limit_usd: null,
  free_primary_cost_limit_usd: null,
  free_weekly_cost_limit_usd: null,
  spending_windows: [
    { duration_seconds: 604800, cost_limit_usd: null },
    { duration_seconds: 18000, cost_limit_usd: null },
  ],
  free_spending_windows: [
    { duration_seconds: 604800, cost_limit_usd: "0" },
    { duration_seconds: 18000, cost_limit_usd: "0" },
  ],
  enabled: true,
  revision: 0,
  updated_at_ms: 0,
};
const limit = (value: string | null) => (value === null ? "不限额" : money(value));
const accessLabel = (mode: "all" | "selected" | "none", models: ModelRef[]) =>
  mode === "all"
    ? "全部已启用模型"
    : mode === "none" || !models.length
      ? "无模型"
      : `指定 ${models.length} 个模型`;
const windowLabel = (seconds: SpendingWindow["duration_seconds"]) =>
  seconds === 2592000 ? "30 天" : seconds === 604800 ? "7 天" : "5 小时";
const windowSummary = (windows?: SpendingWindow[]) =>
  (windows ?? [])
    .map((window) => `${windowLabel(window.duration_seconds)} ${limit(window.cost_limit_usd)}`)
    .join("；") || "未设置";
export default function PlansPage() {
  const fieldId = useId();
  const actions = useActions();
  const resource = useResource<Plans>("/plans");
  const models = useResource<List<Model>>("/models");
  useErrorToast(models.error);
  const [editing, setEditing] = useState<Plan>();
  const empty = { search: "", status: "" };
  const [filters, setFilters] = useState(empty);
  const [applied, setApplied] = useState(empty);
  const items =
    resource.data?.items.filter(
      (plan) =>
        `${plan.name} ${plan.provider_id}`
          .toLowerCase()
          .includes(applied.search.trim().toLowerCase()) &&
        (!applied.status || plan.enabled === (applied.status === "enabled")),
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
                htmlFor={fieldId + "-field-1" + "-" + encodeURIComponent(String("搜索套餐"))}
              >
                {"搜索套餐"}
              </FieldLabel>
              <Input
                id={fieldId + "-field-1" + "-" + encodeURIComponent(String("搜索套餐"))}
                aria-label={"搜索套餐"}
                value={filters.search}
                onChange={(event) => setFilters({ ...filters, search: event.target.value })}
                placeholder="套餐名称或提供商"
              />
            </Field>
            <Field className="w-44">
              <FieldLabel
                htmlFor={fieldId + "-field-2" + "-" + encodeURIComponent(String("分配状态"))}
              >
                {"分配状态"}
              </FieldLabel>
              <Select
                value={filters.status}
                onValueChange={(next) =>
                  ((status) => setFilters({ ...filters, status }))(
                    next ===
                      fieldId + "-field-2" + "-" + encodeURIComponent(String("分配状态")) + "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-2" + "-" + encodeURIComponent(String("分配状态"))}
                  aria-label={"分配状态"}
                  data-required={false ? "true" : undefined}
                  data-empty={String(filters.status) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部状态" },
                        { value: "enabled", label: "可分配" },
                        { value: "disabled", label: "停止新分配" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部状态" },
                    { value: "enabled", label: "可分配" },
                    { value: "disabled", label: "停止新分配" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-2" +
                          "-" +
                          encodeURIComponent(String("分配状态")) +
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
            {
              <Button type="button" onClick={() => setEditing({ ...emptyPlan })}>
                <Plus />
                添加套餐
              </Button>
            }
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="space-y-4">
          {
            <Table>
              <TableHeader>
                <TableRow>
                  {["套餐", "订阅模型", "费用上限", "到期免费访问", "分配状态", "操作"].map(
                    (label) => (
                      <TableHead key={label} scope="col">
                        {label}
                      </TableHead>
                    ),
                  )}
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.length ? (
                  <>
                    {pagination.rows.map((plan) => (
                      <TableRow key={plan.id}>
                        <TableCell>
                          <strong>{plan.name}</strong>
                          <CardDescription>{plan.provider_id}</CardDescription>
                        </TableCell>
                        <TableCell>
                          <span>{accessLabel(plan.model_access, plan.models)}</span>
                          {plan.model_access === "selected" && (
                            <CardDescription className="max-w-80 whitespace-normal break-words">
                              {plan.models.map((model) => model.model).join("、") || "尚未选择"}
                            </CardDescription>
                          )}
                        </TableCell>
                        <TableCell>{windowSummary(plan.spending_windows)}</TableCell>
                        <TableCell>
                          {plan.free_access_enabled
                            ? accessLabel(plan.free_model_access, plan.free_models)
                            : "不开放"}
                          {plan.free_access_enabled && (
                            <CardDescription>
                              {windowSummary(plan.free_spending_windows)}
                            </CardDescription>
                          )}
                        </TableCell>
                        <TableCell>
                          <Badge variant={plan.enabled ? "secondary" : "outline"}>
                            {plan.enabled ? "可分配" : "停止新分配"}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-wrap items-center gap-2">
                            <Button variant="outline" size="sm" onClick={() => setEditing(plan)}>
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
                                  variant={true ? "destructive" : "default"}
                                  disabled={
                                    false || actions.isBusy("app\\plans\\page.tsx:action:3")
                                  }
                                  onSelect={() =>
                                    void actions.run(
                                      "app\\plans\\page.tsx:action:3",
                                      async () => {
                                        await request(`/plans/${plan.id}`, {
                                          method: "DELETE",
                                          body: { revision: plan.revision },
                                        });
                                        resource.reload();
                                      },
                                      {
                                        confirm: "删除此套餐？已分配套餐需先调整，历史记录保留。",
                                        danger: true,
                                        success: "套餐已删除",
                                      },
                                    )
                                  }
                                >
                                  删除套餐
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
                    <TableCell
                      colSpan={
                        ["套餐", "订阅模型", "费用上限", "到期免费访问", "分配状态", "操作"].length
                      }
                    >
                      <Empty>
                        <EmptyDescription>{"暂无符合条件的套餐"}</EmptyDescription>
                      </Empty>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          }
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
      {editing && (
        <PlanEditor
          plan={editing}
          choices={
            models.data?.items
              .filter((model) => model.enabled)
              .map(({ provider_id, model }) => ({ provider_id, model })) ?? []
          }
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
  choices,
  onClose,
  onSaved,
}: {
  plan: Plan;
  choices: ModelRef[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const dialogFocus = useDialogFocus();

  const fieldId = useId();
  const actions = useActions();
  const [value, setValue] = useState(plan);
  const update = <K extends keyof Plan>(key: K, next: Plan[K]) =>
    setValue((current) => ({ ...current, [key]: next }));
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
          <DialogTitle>{plan.id ? "编辑套餐" : "添加套餐"}</DialogTitle>
          <DialogDescription>{"配置套餐的模型权限和费用窗口。"}</DialogDescription>
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
          aria-busy={actions.isBusy("app\\plans\\page.tsx:form:5")}
          onSubmit={(event) =>
            actions.submit(
              event,
              "app\\plans\\page.tsx:form:5",
              async () => {
                await request(plan.id ? `/plans/${plan.id}` : "/plans", {
                  method: plan.id ? "PUT" : "POST",
                  body: planWrite(value),
                });
                onSaved();
              },
              "已保存",
            )
          }
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={actions.isBusy("app\\plans\\page.tsx:form:5")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      基本信息
                    </CardTitle>
                  </div>
                  <div className="grid sm:grid-cols-2 gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId + "-field-6" + "-" + encodeURIComponent(String("套餐名称"))
                        }
                      >
                        {"套餐名称"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-6" + "-" + encodeURIComponent(String("套餐名称"))}
                        aria-label={"套餐名称"}
                        value={value.name}
                        onChange={(event) => update("name", event.target.value)}
                        required
                        maxLength={128}
                      />
                    </Field>
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
                        disabled={Boolean(plan.id)}
                      >
                        <SelectTrigger
                          id={fieldId + "-field-7" + "-" + encodeURIComponent(String("提供商"))}
                          aria-label={"提供商"}
                          aria-describedby={
                            fieldId +
                            "-field-7" +
                            "-" +
                            encodeURIComponent(String("提供商")) +
                            "-hint"
                          }
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
                      {Boolean(plan.id ? "创建后固定" : undefined) && (
                        <FieldDescription
                          id={
                            fieldId +
                            "-field-7" +
                            "-" +
                            encodeURIComponent(String("提供商")) +
                            "-hint"
                          }
                        >
                          {plan.id ? "创建后固定" : undefined}
                        </FieldDescription>
                      )}
                    </Field>
                  </div>
                  <Field orientation="horizontal">
                    <Switch
                      id={
                        fieldId + "-field-8" + "-" + encodeURIComponent(String("允许新分配此套餐"))
                      }
                      checked={value.enabled}
                      onCheckedChange={(enabled) => update("enabled", enabled)}
                    />
                    <div>
                      <FieldLabel
                        htmlFor={
                          fieldId +
                          "-field-8" +
                          "-" +
                          encodeURIComponent(String("允许新分配此套餐"))
                        }
                      >
                        {"允许新分配此套餐"}
                      </FieldLabel>
                    </div>
                  </Field>
                </section>
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      订阅费用窗口
                    </CardTitle>
                    <CardDescription>
                      周期额度可选 7 天或 30 天，也可配置 5 小时额度。
                    </CardDescription>
                  </div>
                  <WindowEditor
                    label="订阅额度"
                    windows={value.spending_windows}
                    onChange={(windows) => update("spending_windows", windows)}
                    id={fieldId + "-paid-windows"}
                  />
                </section>
                <ModelAccess
                  title="订阅模型范围"
                  mode={value.model_access}
                  selected={value.models}
                  choices={choices}
                  provider={value.provider_id}
                  onMode={(mode) => update("model_access", mode === "none" ? "selected" : mode)}
                  onModels={(models) => update("models", models)}
                />
                <Collapsible defaultOpen={value.free_access_enabled} className="space-y-3">
                  <CollapsibleTrigger asChild>
                    <Button type="button" variant="ghost" className="w-full justify-between">
                      <span>到期免费访问</span>
                      <span className="text-sm text-muted-foreground">
                        {value.free_access_enabled ? "已开放" : "未开放"}
                      </span>
                      <ChevronDown />
                    </Button>
                  </CollapsibleTrigger>
                  <CollapsibleContent className="space-y-4">
                    <Field orientation="horizontal">
                      <Switch
                        id={
                          fieldId +
                          "-field-9" +
                          "-" +
                          encodeURIComponent(String("订阅到期后允许使用免费层"))
                        }
                        checked={value.free_access_enabled}
                        onCheckedChange={(enabled) => update("free_access_enabled", enabled)}
                      />
                      <div>
                        <FieldLabel
                          htmlFor={
                            fieldId +
                            "-field-9" +
                            "-" +
                            encodeURIComponent(String("订阅到期后允许使用免费层"))
                          }
                        >
                          {"订阅到期后允许使用免费层"}
                        </FieldLabel>
                      </div>
                    </Field>
                    <CardDescription className="text-sm text-muted-foreground">
                      订阅到期结束付费权益，登录状态由账户启停独立控制。关闭免费访问时，下方设置仍会保留。
                    </CardDescription>
                    <WindowEditor
                      label="免费层额度"
                      windows={value.free_spending_windows}
                      onChange={(windows) => update("free_spending_windows", windows)}
                      id={fieldId + "-free-windows"}
                    />
                    <ModelAccess
                      title="免费层模型范围"
                      allowNone
                      mode={value.free_model_access}
                      selected={value.free_models}
                      choices={choices}
                      provider={value.provider_id}
                      onMode={(mode) => update("free_model_access", mode)}
                      onModels={(models) => update("free_models", models)}
                    />
                  </CollapsibleContent>
                </Collapsible>
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
            {onClose && (
              <Button
                type="button"
                variant="outline"
                disabled={actions.isBusy("app\\plans\\page.tsx:form:5")}
                onClick={onClose}
              >
                {"取消"}
              </Button>
            )}
            <Button type="submit" disabled={actions.isBusy("app\\plans\\page.tsx:form:5")}>
              {actions.isBusy("app\\plans\\page.tsx:form:5") && <Spinner />}
              {actions.isBusy("app\\plans\\page.tsx:form:5") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </DialogContent>
    </Dialog>
  );
}
function WindowEditor({
  label,
  windows,
  onChange,
  id,
}: {
  label: string;
  windows: SpendingWindow[];
  onChange: (windows: SpendingWindow[]) => void;
  id: string;
}) {
  const configured = windows ?? [];
  const outer = configured[0] ?? { duration_seconds: 604800, cost_limit_usd: null };
  const inner = configured[1];
  const updateOuter = (patch: Partial<SpendingWindow>) =>
    onChange([{ ...outer, ...patch }, ...(inner ? [inner] : [])]);
  const updateInner = (patch: Partial<SpendingWindow>) =>
    onChange([
      outer,
      { ...(inner ?? { duration_seconds: 18000, cost_limit_usd: null }), ...patch },
    ]);
  return (
    <div className="space-y-3 rounded-lg border p-3">
      <div className="grid gap-3 sm:grid-cols-[10rem_1fr]">
        <Field>
          <FieldLabel htmlFor={`${id}-outer-duration`}>{label}周期</FieldLabel>
          <Select
            value={String(outer.duration_seconds)}
            onValueChange={(next) =>
              updateOuter({ duration_seconds: Number(next) as 604800 | 2592000 })
            }
          >
            <SelectTrigger id={`${id}-outer-duration`} aria-label={`${label}周期`}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="604800">7 天</SelectItem>
              <SelectItem value="2592000">30 天</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-outer-limit`}>{label}周期费用上限（美元）</FieldLabel>
          <Input
            id={`${id}-outer-limit`}
            aria-label={`${label}周期费用上限（美元）`}
            type="number"
            min="0"
            step="0.000000001"
            value={outer.cost_limit_usd ?? ""}
            onChange={(event) => updateOuter({ cost_limit_usd: event.target.value || null })}
          />
          <FieldDescription>留空表示周期额度不限额。</FieldDescription>
        </Field>
      </div>
      <div className="flex items-center gap-2">
        <Switch
          id={`${id}-inner-enabled`}
          aria-label={`${label} 5 小时额度`}
          checked={Boolean(inner)}
          onCheckedChange={(enabled) =>
            onChange(
              enabled
                ? [outer, inner ?? { duration_seconds: 18000, cost_limit_usd: null }]
                : [outer],
            )
          }
        />
        <FieldLabel htmlFor={`${id}-inner-enabled`}>启用 5 小时额度</FieldLabel>
      </div>
      {inner && (
        <Field>
          <FieldLabel htmlFor={`${id}-inner-limit`}>{label} 5 小时费用上限（美元）</FieldLabel>
          <Input
            id={`${id}-inner-limit`}
            aria-label={`${label} 5 小时费用上限（美元）`}
            type="number"
            min="0"
            step="0.000000001"
            value={inner.cost_limit_usd ?? ""}
            onChange={(event) => updateInner({ cost_limit_usd: event.target.value || null })}
          />
          <FieldDescription>从第一次使用开始计时，受周期额度剩余值限制。</FieldDescription>
        </Field>
      )}
    </div>
  );
}

export function ModelAccess({
  title,
  mode,
  selected,
  choices,
  provider,
  allowNone,
  onMode,
  onModels,
}: {
  title: string;
  mode: "all" | "selected" | "none";
  selected: ModelRef[];
  choices: ModelRef[];
  provider: string;
  allowNone?: boolean;
  onMode: (mode: "all" | "selected" | "none") => void;
  onModels: (models: ModelRef[]) => void;
}) {
  const fieldId = useId();
  const id = useId();
  const [search, setSearch] = useState("");
  const available = [
    ...new Map(
      [...sameProviderModels(choices, provider), ...sameProviderModels(selected, provider)].map(
        (model) => [modelKey(model), model],
      ),
    ).values(),
  ];
  const filtered = available.filter((model) =>
    modelKey(model).toLowerCase().includes(search.trim().toLowerCase()),
  );
  const selectedKeys = new Set(selected.map(modelKey));
  const selectedCount = sameProviderModels(selected, provider).length;
  return (
    <FieldSet className="space-y-3">
      <FieldLegend>{title}</FieldLegend>
      <Field>
        <FieldLabel htmlFor={fieldId + "-field-10" + "-" + encodeURIComponent(String("访问范围"))}>
          {"访问范围"}
        </FieldLabel>
        <Select
          value={mode}
          onValueChange={(next) =>
            ((next) => onMode(next as typeof mode))(
              next ===
                fieldId + "-field-10" + "-" + encodeURIComponent(String("访问范围")) + "-empty"
                ? ""
                : next,
            )
          }
        >
          <SelectTrigger
            id={fieldId + "-field-10" + "-" + encodeURIComponent(String("访问范围"))}
            aria-label={"访问范围"}
            data-required={false ? "true" : undefined}
            data-empty={String(mode) === "" ? "true" : undefined}
            className="w-full"
          >
            <SelectValue
              placeholder={
                [
                  ...(allowNone ? [{ value: "none", label: "无模型" }] : []),
                  { value: "all", label: "全部已启用模型" },
                  { value: "selected", label: "指定模型" },
                ].find((option) => option.value === "")?.label ?? "请选择"
              }
            />
          </SelectTrigger>
          <SelectContent position="popper">
            {[
              ...(allowNone ? [{ value: "none", label: "无模型" }] : []),
              { value: "all", label: "全部已启用模型" },
              { value: "selected", label: "指定模型" },
            ].map((option) => (
              <SelectItem
                key={option.value}
                value={
                  option.value ||
                  fieldId + "-field-10" + "-" + encodeURIComponent(String("访问范围")) + "-empty"
                }
                disabled={"disabled" in option && Boolean(option.disabled)}
              >
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      {mode === "all" && (
        <CardDescription className="text-sm text-muted-foreground">
          允许此提供商的全部已启用模型，后续启用的模型也包含在内。
        </CardDescription>
      )}
      {mode === "none" && (
        <CardDescription className="text-sm text-muted-foreground">
          此范围不授予任何模型权限。
        </CardDescription>
      )}
      {mode === "selected" && (
        <>
          <div className="flex items-end justify-between gap-3">
            <Field>
              <FieldLabel
                htmlFor={fieldId + "-field-11" + "-" + encodeURIComponent(String("搜索模型"))}
              >
                {"搜索模型"}
              </FieldLabel>
              <Input
                id={fieldId + "-field-11" + "-" + encodeURIComponent(String("搜索模型"))}
                aria-label={"搜索模型"}
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="输入模型名称"
              />
            </Field>
            <span className="text-sm text-muted-foreground">
              已选 {selectedCount} / {available.length} 个
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={
                !filtered.length || filtered.every((model) => selectedKeys.has(modelKey(model)))
              }
              onClick={() =>
                onModels([
                  ...new Map(
                    [...selected, ...filtered].map((model) => [modelKey(model), model]),
                  ).values(),
                ])
              }
            >
              选择当前结果
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={!filtered.some((model) => selectedKeys.has(modelKey(model)))}
              onClick={() => {
                const keys = new Set(filtered.map(modelKey));
                onModels(selected.filter((model) => !keys.has(modelKey(model))));
              }}
            >
              清除当前结果
            </Button>
            <span className="text-sm text-muted-foreground">当前显示 {filtered.length} 个</span>
          </div>
          <div className="grid max-h-64 gap-2 overflow-y-auto sm:grid-cols-2">
            {filtered.length ? (
              filtered.map((model, index) => (
                <Label key={modelKey(model)} htmlFor={`${id}-${index}`}>
                  <Checkbox
                    id={`${id}-${index}`}
                    checked={selectedKeys.has(modelKey(model))}
                    onCheckedChange={(checked) =>
                      onModels(
                        checked === true
                          ? [
                              ...selected.filter((item) => modelKey(item) !== modelKey(model)),
                              model,
                            ]
                          : selected.filter((item) => modelKey(item) !== modelKey(model)),
                      )
                    }
                  />
                  <span>{modelKey(model)}</span>
                </Label>
              ))
            ) : (
              <Empty>
                <EmptyHeader>
                  <EmptyMedia variant="icon">
                    <Inbox />
                  </EmptyMedia>
                  <EmptyDescription>
                    {available.length ? "没有匹配的模型" : "暂无模型，请先在模型配置中添加。"}
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            )}
          </div>
          {!selectedCount && (
            <CardDescription className="text-sm text-muted-foreground">
              尚未选择模型。保存后，此范围不允许任何模型请求。
            </CardDescription>
          )}
        </>
      )}
    </FieldSet>
  );
}
