"use client";
import { ModelEditor } from "@/app/components/models/model-editor";
import { ModelRecordDialog } from "@/app/components/models/model-record-dialog";
import { emptyModel, useModelPricing } from "@/app/data/models";
import { GrokModelSync } from "@/components/providers/grok/models";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Empty, EmptyDescription } from "@/components/ui/empty";
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
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  MoreHorizontal,
  Pencil,
  Plus,
  RotateCcw,
  Search,
} from "lucide-react";

export function ModelPricing() {
  const {
    tableColumns0,
    fieldId,
    actions,
    presets,
    editing,
    setEditing,
    empty,
    filters,
    setFilters,
    setApplied,
    resource,
    pagination,
    handleSelect,
    handleSelect2,
  } = useModelPricing();
  return (
    <>
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters });
              resource.reload(1);
            }}
          >
            <Field className="w-40">
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
            <Field className="w-40">
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
            <Field className="w-40">
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
                  resource.reload(1);
                }}
              >
                <RotateCcw />
                重置
              </Button>
            </div>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button type="button" variant="outline" size="sm" aria-label="显示列">
                  <Columns3 />
                  显示列
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-48">
                <DropdownMenuLabel>
                  {tableColumns0.mobile ? "手机显示列" : "桌面显示列"}
                </DropdownMenuLabel>
                {tableColumns0.labels.map((label) => (
                  <DropdownMenuCheckboxItem
                    key={label}
                    checked={tableColumns0.isVisible(label)}
                    disabled={tableColumns0.count === 1 && tableColumns0.isVisible(label)}
                    onSelect={(event) => event.preventDefault()}
                    onCheckedChange={(checked) => tableColumns0.setVisible(label, checked === true)}
                  >
                    {label}
                  </DropdownMenuCheckboxItem>
                ))}
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={tableColumns0.showAll}>显示全部列</DropdownMenuItem>
                <DropdownMenuItem onSelect={tableColumns0.reset}>恢复默认列</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </form>
          <div className="flex flex-wrap items-center gap-2 self-end xl:ml-auto">
            <GrokModelSync
              onSynced={() => {
                resource.reload();
                presets.reload();
              }}
            />
            {(resource.error || presets.error) && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => {
                  resource.reload();
                  presets.reload();
                }}
              >
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
            <Table
              className={
                tableColumns0.count > 4
                  ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
              }
              role="table"
            >
              <TableHeader>
                <TableRow role="row">
                  {["模型", "计费方式", "价格规则", "状态", "操作"].map((label) => (
                    <TableHead
                      hidden={!tableColumns0.isVisible(label)}
                      className={
                        ["模型", "状态", "操作"].includes(label)
                          ? label === "操作"
                            ? "max-md:w-28"
                            : label === "模型"
                              ? ""
                              : "max-md:w-16"
                          : ""
                      }
                      key={label}
                      scope="col"
                    >
                      {label}
                    </TableHead>
                  ))}
                </TableRow>
              </TableHeader>
              <TableBody>
                {pagination.rows.length ? (
                  <>
                    {pagination.rows.map((model) => (
                      <TableRow role="row" key={`${model.provider_id}/${model.model}`}>
                        <TableCell
                          hidden={!tableColumns0.isVisible("模型")}
                          className=" max-md:overflow-hidden"
                          data-label="模型"
                          role="cell"
                        >
                          <div className="max-md:hidden">
                            <strong>{model.model}</strong>
                            <CardDescription>{model.provider_id}</CardDescription>
                          </div>
                          <ModelRecordDialog model={model} />
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("计费方式")}
                          className=" "
                          data-label="计费方式"
                          data-compact="true"
                          role="cell"
                        >
                          {model.kind === "text" ? "文本 · Token" : "图像 · 按张"}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("价格规则")}
                          className=" "
                          data-label="价格规则"
                          role="cell"
                        >
                          {model.kind === "text" ? (
                            <>
                              <span>
                                {model.token_prices.length
                                  ? `${model.token_prices.length} 条价格规则`
                                  : "待定价"}
                              </span>
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
                        <TableCell
                          hidden={!tableColumns0.isVisible("状态")}
                          className=" max-md:overflow-hidden"
                          data-label="状态"
                          data-compact="true"
                          role="cell"
                        >
                          <Badge variant={model.enabled ? "secondary" : "outline"}>
                            {model.enabled ? "已启用" : "已停用"}
                          </Badge>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("操作")}
                          className=" max-md:[&_button]:h-7 max-md:[&_button]:px-1.5 max-md:[&_button]:text-xs max-md:[&_button]:gap-1 max-md:[&_a]:h-7 max-md:[&_a]:px-1.5 max-md:[&_a]:text-xs max-md:[&_a]:gap-1 max-md:[&>div]:gap-1"
                          data-label="操作"
                          role="cell"
                        >
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
                                  variant="default"
                                  disabled={
                                    false || actions.isBusy("app\\models\\page.tsx:action:4")
                                  }
                                  onSelect={() => handleSelect(model)}
                                >
                                  {model.enabled ? "停用" : "启用"}
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  variant="destructive"
                                  disabled={
                                    false || actions.isBusy("app\\models\\page.tsx:action:5")
                                  }
                                  onSelect={() => handleSelect2(model)}
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
                  <TableRow role="row">
                    <TableCell role="cell" colSpan={tableColumns0.count}>
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
          presets={presets.data?.items}
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
