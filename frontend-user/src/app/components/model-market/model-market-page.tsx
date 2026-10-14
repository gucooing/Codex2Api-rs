"use client";
import { ModelGroupLimits } from "./model-group-limits";
import { ModelMarketCard } from "./model-market-card";
import { ModelPriceDialog } from "./model-price-dialog";
import { SearchBillingCard } from "@/components/providers/chatgpt/search-billing-card";
import { useModelMarketPage } from "@/app/data/model-market";
import { Button } from "@/components/ui/button";
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
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
    currentGroup,
    showOtherBilling,
    searchPrice,
    selectGroup,
    filters,
    setFilters,
    resource,
    items,
    pagination,
    selected,
    setSelected,
    search,
    reset,
    reload,
  } = useModelMarketPage();
  return (
    <>
      <Tabs value={groupId || "base"} onValueChange={selectGroup}>
        <div className="overflow-x-auto overflow-y-hidden">
          <TabsList className="w-max">
            <TabsTrigger value="base">基础计费</TabsTrigger>
            {groups.data?.items.map((group) => (
              <TabsTrigger key={group.id} value={group.id}>
                {group.name}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        <TabsContent value={groupId || "base"} className="space-y-4">
          <form className="flex flex-wrap items-end gap-3" onSubmit={search}>
            <Field className="min-w-32 flex-1 sm:w-48 sm:flex-none">
              <FieldLabel htmlFor={id + "-search"}>模型</FieldLabel>
              <Input
                id={id + "-search"}
                value={filters.search}
                placeholder="搜索模型名称"
                onChange={(event) => setFilters({ ...filters, search: event.target.value })}
              />
            </Field>
            <Field className="w-32 sm:w-36">
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
          {!!groupId && (
            <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
              {currentGroup?.description && <span>{currentGroup.description}</span>}
              <ModelGroupLimits group={currentGroup} />
            </div>
          )}
          <section aria-label="模型列表" aria-busy={!resource.ready && !resource.error}>
            <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
              {items.map((item) => (
                <li key={`${item.model.provider_id}/${item.model.model}`} className="min-w-0">
                  <ModelMarketCard
                    item={item}
                    grouped={!!groupId}
                    onSelect={() => setSelected(item)}
                  />
                </li>
              ))}
              {showOtherBilling && (
                <li className="min-w-0">
                  <SearchBillingCard price={searchPrice} grouped={!!groupId} />
                </li>
              )}
            </ul>
            {resource.ready && items.length === 0 && !showOtherBilling && (
              <Empty>
                <EmptyHeader>
                  <EmptyTitle>暂无匹配项目</EmptyTitle>
                </EmptyHeader>
              </Empty>
            )}
          </section>
          <Pagination className="justify-end" aria-label="模型分页">
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
                共 {pagination.total ?? "—"} 个模型 · {pagination.pages ?? "—"} 页
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" size="icon-sm" aria-label="首页" {...pagination.first}>
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
                <Button variant="outline" size="icon-sm" aria-label="下一页" {...pagination.next}>
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
        </TabsContent>
      </Tabs>
      <ModelPriceDialog
        item={selected}
        group={currentGroup}
        onClose={() => setSelected(undefined)}
      />
    </>
  );
}
