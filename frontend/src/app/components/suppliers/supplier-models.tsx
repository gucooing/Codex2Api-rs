"use client";
import { useSupplierModels } from "@/app/data/suppliers";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Spinner } from "@/components/ui/spinner";
import { Input } from "@/components/ui/input";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, RefreshCw } from "lucide-react";
import { useId } from "react";

export function SupplierModels({ id }: { id: string }) {
  const fieldId = useId();
  const {
    resource,
    options,
    choices,
    selected,
    setSelected,
    setSearch,
    busy,
    syncing,
    ready,
    reload,
    sync,
    remove,
  } = useSupplierModels(id);
  const { pagination } = resource;
  return (
    <Card size="sm">
      <CardContent className="space-y-3">
        <div className="flex flex-wrap items-end gap-2">
          <Field className="w-full sm:w-80">
            <FieldLabel htmlFor={fieldId}>添加自定义模型</FieldLabel>
            <Combobox
              multiple
              items={choices}
              value={selected}
              onValueChange={setSelected}
              filter={null}
              onInputValueChange={(text, details) => {
                if (details.reason === "input-change" || details.reason === "input-clear")
                  setSearch(text);
              }}
            >
              <ComboboxInput id={fieldId} placeholder="选择或输入模型名称" />
              <ComboboxContent>
                <ComboboxEmpty>{options.loading ? "加载中…" : "没有匹配模型"}</ComboboxEmpty>
                <ComboboxList>
                  {(item) => (
                    <ComboboxItem
                      key={item}
                      value={item}
                      disabled={!ready || !options.ready || busy}
                    >
                      {item}
                    </ComboboxItem>
                  )}
                </ComboboxList>
              </ComboboxContent>
            </Combobox>
          </Field>
          <Button size="sm" variant="outline" disabled={!ready || busy} onClick={sync}>
            {syncing && <Spinner />}同步上游模型
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={reload}
            disabled={resource.refreshing || busy}
          >
            <RefreshCw />
            刷新
          </Button>
        </div>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>模型</TableHead>
              <TableHead>来源</TableHead>
              <TableHead className="w-24 text-right">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {(resource.data?.items ?? []).map((item) => (
              <TableRow key={item.model}>
                <TableCell className="font-medium">{item.model}</TableCell>
                <TableCell>
                  <Badge variant={item.source === "upstream" ? "secondary" : "outline"}>
                    {item.source === "upstream" ? "上游同步" : "自定义"}
                  </Badge>
                </TableCell>
                <TableCell className="text-right">
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={!ready || busy}
                    onClick={() => remove(item.model)}
                  >
                    移除
                  </Button>
                </TableCell>
              </TableRow>
            ))}
            {!resource.data?.items.length && (
              <TableRow>
                <TableCell colSpan={3} className="text-center text-muted-foreground">
                  {resource.loading ? "加载中…" : "暂无可用模型"}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
        <Pagination className="mt-3 justify-end" aria-label="供应模型分页">
          <PaginationContent className="flex-wrap justify-end gap-1">
            <PaginationItem>
              <Select {...pagination.size}>
                <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
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
              <Button variant="outline" size="icon-sm" aria-label="首页" {...pagination.first}>
                <ChevronsLeft />
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Button variant="outline" size="icon-sm" aria-label="上一页" {...pagination.previous}>
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
      </CardContent>
    </Card>
  );
}
