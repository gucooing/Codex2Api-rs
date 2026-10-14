"use client";
import { useUsagePage } from "@/app/data/usage";
import { UsageTable } from "./usage-table";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
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
import { usageStatuses } from "@/lib/usage-display";
import { usageCost } from "@/lib/usage";
import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  RotateCcw,
  Search,
} from "lucide-react";

export default function UsagePage() {
  const { filters, id, resource, pagination, update, search, reset } = useUsagePage();
  return (
    <>
      <Card>
        <CardContent className="space-y-4">
          <form className="space-y-4" noValidate onSubmit={search}>
            <Collapsible>
              <div className="flex flex-wrap items-end gap-3">
                <Field className="w-40">
                  <FieldLabel htmlFor={`${id}-days`}>时间范围</FieldLabel>
                  <Select value={filters.days} onValueChange={(value) => update("days", value)}>
                    <SelectTrigger id={`${id}-days`} className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="1">今天</SelectItem>
                      <SelectItem value="7">最近 7 天</SelectItem>
                      <SelectItem value="30">最近 30 天</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                <Field className="w-40">
                  <FieldLabel htmlFor={`${id}-provider`}>平台</FieldLabel>
                  <Select
                    value={filters.provider}
                    onValueChange={(value) => update("provider", value)}
                  >
                    <SelectTrigger id={`${id}-provider`} className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="all">全部平台</SelectItem>
                      <SelectItem value="chatgpt">ChatGPT</SelectItem>
                      <SelectItem value="grok">Grok</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                <Field className="w-40">
                  <FieldLabel htmlFor={`${id}-model`}>模型</FieldLabel>
                  <Input
                    id={`${id}-model`}
                    value={filters.model}
                    maxLength={100}
                    onChange={(event) => update("model", event.target.value)}
                  />
                </Field>
                <Field className="w-40">
                  <FieldLabel htmlFor={`${id}-status`}>状态</FieldLabel>
                  <Select value={filters.status} onValueChange={(value) => update("status", value)}>
                    <SelectTrigger id={`${id}-status`} className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="all">全部状态</SelectItem>
                      {usageStatuses.map((status) => (
                        <SelectItem key={status.value} value={status.value}>
                          {status.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </Field>
                <Field className="w-40">
                  <FieldLabel htmlFor={id + "-source"}>来源</FieldLabel>
                  <Select value={filters.source} onValueChange={(next) => update("source", next)}>
                    <SelectTrigger id={id + "-source"} className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="all">全部来源</SelectItem>
                      <SelectItem value="api">Api</SelectItem>
                      <SelectItem value="subscription">订阅</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                <div className="flex flex-wrap items-center gap-2 self-end">
                  <Button type="submit">
                    <Search />
                    查询
                  </Button>
                  <Button type="button" variant="outline" onClick={reset}>
                    <RotateCcw />
                    重置
                  </Button>
                  <CollapsibleTrigger asChild>
                    <Button type="button" variant="ghost">
                      <CalendarDays />
                      起止时间{filters.from || filters.until ? " · 已设置" : ""}
                    </Button>
                  </CollapsibleTrigger>
                </div>
              </div>
              <CollapsibleContent>
                <div className="mt-3 flex flex-wrap items-end gap-3">
                  <Field className="w-56">
                    <FieldLabel htmlFor={`${id}-from`}>开始时间</FieldLabel>
                    <Input
                      id={`${id}-from`}
                      type="datetime-local"
                      value={filters.from}
                      max={filters.until || undefined}
                      onChange={(event) => update("from", event.target.value)}
                    />
                  </Field>
                  <Field className="w-56">
                    <FieldLabel htmlFor={`${id}-until`}>结束时间</FieldLabel>
                    <Input
                      id={`${id}-until`}
                      type="datetime-local"
                      value={filters.until}
                      min={filters.from || undefined}
                      onChange={(event) => update("until", event.target.value)}
                    />
                  </Field>
                </div>
              </CollapsibleContent>
            </Collapsible>
          </form>
        </CardContent>
      </Card>
      <Card>
        <CardContent className="space-y-4">
          <p className="text-xs text-muted-foreground">
            钱包待扣费用：{usageCost(resource.data?.wallet_pending_nano_usd)}
          </p>
          <UsageTable records={resource.data?.items ?? []} />
          <Pagination aria-label="记录分页" className="justify-end">
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
    </>
  );
}
