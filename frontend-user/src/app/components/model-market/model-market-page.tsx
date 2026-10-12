"use client";
import { ModelPriceDialog } from "@/app/components/model-market/model-price-dialog";
import { useModelMarketPage } from "@/app/data/model-market";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { apiLimit, apiPlatform, apiPrice, apiRange, apiTier } from "@/lib/metered-api";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  RefreshCw,
  RotateCcw,
  Search,
} from "lucide-react";
export default function ModelMarketPage() {
  const {
    id,
    groups,
    groupId,
    selectGroup,
    filters,
    setFilters,
    resource,
    pagination,
    selected,
    setSelected,
    search,
    reset,
    reload,
  } = useModelMarketPage();
  const currentGroup = resource.data?.group;
  return (
    <>
      <Tabs value={groupId || "base"} onValueChange={selectGroup}>
        <div className="overflow-x-auto">
          <TabsList className="w-max">
            <TabsTrigger value="base">基础计费目录</TabsTrigger>
            {groups.data?.items.map((group) => (
              <TabsTrigger key={group.id} value={group.id}>
                {group.name}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        <TabsContent value={groupId || "base"} className="space-y-4">
          <form className="flex flex-wrap items-end gap-3" onSubmit={search}>
            <Field className="w-48">
              <FieldLabel htmlFor={id + "-search"}>模型</FieldLabel>
              <Input
                id={id + "-search"}
                value={filters.search}
                placeholder="搜索模型名称"
                onChange={(event) => setFilters({ ...filters, search: event.target.value })}
              />
            </Field>
            <Field className="w-36">
              <FieldLabel htmlFor={id + "-provider"}>平台</FieldLabel>
              <Select
                value={filters.provider_id || "all"}
                onValueChange={(provider) =>
                  setFilters({ ...filters, provider_id: provider === "all" ? "" : provider })
                }
              >
                <SelectTrigger id={id + "-provider"} className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="all">全部平台</SelectItem>
                  <SelectItem value="chatgpt">ChatGPT</SelectItem>
                  <SelectItem value="grok">Grok</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <Button type="submit">
              <Search />
              查询
            </Button>
            <Button type="button" variant="secondary" onClick={reset}>
              <RotateCcw />
              重置
            </Button>
            <Button
              type="button"
              className="ml-auto"
              variant="ghost"
              size="icon-sm"
              aria-label="刷新模型目录"
              onClick={reload}
            >
              <RefreshCw />
            </Button>
          </form>
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <Badge variant="secondary">
              {groupId ? (currentGroup?.name ?? "API 组") : "基础价格"}
            </Badge>
            {groupId && (
              <>
                <span>组倍率 {currentGroup?.multiplier ?? "—"}×</span>
                <span>
                  每用户组并发 {currentGroup ? apiLimit(currentGroup.concurrency_limit) : "—"}
                </span>
                <span>组 RPM {currentGroup ? apiLimit(currentGroup.rpm_limit) : "—"}</span>
              </>
            )}
            <span className="text-muted-foreground">
              {groupId
                ? "同一用户的所有密钥共同计数，组与模型限制同时生效。"
                : "API 按所选组倍率从钱包按量计费。"}
            </span>
          </div>
          {currentGroup?.description && (
            <p className="text-sm text-muted-foreground">{currentGroup.description}</p>
          )}
          <Card>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>模型</TableHead>
                    <TableHead>倍率</TableHead>
                    <TableHead>价格（USD）</TableHead>
                    <TableHead className="max-md:hidden">每用户模型限制</TableHead>
                    <TableHead>详情</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {resource.data?.items.map((model) => (
                    <TableRow key={model.provider_id + "/" + model.model}>
                      <TableCell>
                        <p className="max-w-48 truncate font-medium" title={model.model}>
                          {model.model}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {apiPlatform(model.provider_id)} ·{" "}
                          {model.kind === "image" ? "图像" : "文本"}
                        </p>
                      </TableCell>
                      <TableCell>{model.multiplier}×</TableCell>
                      <TableCell>
                        {model.kind === "image"
                          ? model.image_prices.length
                            ? model.image_prices.map((price) => (
                                <div key={price.resolution} className="text-xs">
                                  <span className="text-muted-foreground">{price.resolution}</span>{" "}
                                  {apiPrice(price.base_price_usd)} → {apiPrice(price.price_usd)} /
                                  张
                                </div>
                              ))
                            : "待定价"
                          : model.prices.length
                            ? model.prices.map((price, index) => (
                                <div key={index} className="text-xs">
                                  <p className="text-muted-foreground">
                                    {apiTier(price.tier)} · {apiRange(price)} 输入 Token
                                  </p>
                                  <p>
                                    输入 {apiPrice(price.base_input_usd_per_million)} →{" "}
                                    {apiPrice(price.input_usd_per_million)}
                                  </p>
                                  <p>
                                    输出 {apiPrice(price.base_output_usd_per_million)} →{" "}
                                    {apiPrice(price.output_usd_per_million)}
                                  </p>
                                </div>
                              ))
                            : "待定价"}
                        <p className="text-xs text-muted-foreground">
                          {model.kind === "text"
                            ? "每百万 Token · 基础价 → 计费价"
                            : "基础价 → 计费价"}
                        </p>
                      </TableCell>
                      <TableCell className="max-md:hidden">
                        {groupId ? (
                          <>
                            <p>并发 {apiLimit(model.concurrency_limit)}</p>
                            <p>RPM {apiLimit(model.rpm_limit)}</p>
                          </>
                        ) : (
                          "按 API 组规则"
                        )}
                      </TableCell>
                      <TableCell>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => setSelected(model)}
                        >
                          查看
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                  {resource.ready && !resource.data?.items.length && (
                    <TableRow>
                      <TableCell colSpan={5} className="text-center text-muted-foreground">
                        暂无匹配模型
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
              <Pagination className="mt-3 justify-end" aria-label="模型分页">
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
                  <PaginationItem className="text-xs text-muted-foreground">
                    共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
                  </PaginationItem>
                  <PaginationItem>
                    <Button
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
                      variant="outline"
                      size="icon-sm"
                      aria-label="下一页"
                      {...pagination.next}
                    >
                      <ChevronRight />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Button variant="outline" size="icon-sm" aria-label="末页" {...pagination.last}>
                      <ChevronsRight />
                    </Button>
                  </PaginationItem>
                </PaginationContent>
              </Pagination>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
      <ModelPriceDialog
        model={selected}
        grouped={!!groupId}
        onClose={() => setSelected(undefined)}
      />
    </>
  );
}
