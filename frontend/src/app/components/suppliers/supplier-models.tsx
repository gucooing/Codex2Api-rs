"use client";
import { useSupplierModels } from "@/app/data/suppliers";
import { Button } from "@/components/ui/button";
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
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from "lucide-react";
import { useId } from "react";
export function SupplierModels({ id }: { id: string }) {
  const fieldId = useId();
  const { resource, options, selected, setSelected, setSearch, actions, sync, add, remove } =
    useSupplierModels(id);
  const { pagination } = resource;
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-end gap-2">
        <Field className="w-full sm:w-80">
          <FieldLabel htmlFor={fieldId}>添加自定义模型</FieldLabel>
          <Combobox
            items={options.data?.items ?? []}
            value={selected}
            onValueChange={setSelected}
            filter={null}
            itemToStringLabel={(item) => item.model}
            itemToStringValue={(item) => item.model}
            isItemEqualToValue={(a, b) => a.model === b.model}
            onInputValueChange={(text, details) => {
              if (details.reason === "input-change") setSearch(text);
            }}
          >
            <ComboboxInput id={fieldId} placeholder="搜索计费配置中的模型" />
            <ComboboxContent>
              <ComboboxEmpty>{options.loading ? "加载中…" : "没有匹配模型"}</ComboboxEmpty>
              <ComboboxList>
                {(item) => (
                  <ComboboxItem key={item.model} value={item}>
                    {item.model}
                    {item.enabled ? "" : "（未启用）"}
                  </ComboboxItem>
                )}
              </ComboboxList>
            </ComboboxContent>
          </Combobox>
        </Field>
        <Button
          disabled={!resource.ready || !options.ready || !selected || actions.isBusy("add-model")}
          onClick={add}
        >
          添加
        </Button>
        <Button
          variant="outline"
          disabled={!resource.ready || actions.isBusy("sync-models")}
          onClick={sync}
        >
          {actions.isBusy("sync-models") && <Spinner />}同步上游模型
        </Button>
        <Button variant="outline" onClick={resource.reload} disabled={resource.refreshing}>
          刷新
        </Button>
      </div>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>模型</TableHead>
            <TableHead>来源</TableHead>
            <TableHead>操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {(resource.data?.items ?? []).map((item) => (
            <TableRow key={item.model}>
              <TableCell>{item.model}</TableCell>
              <TableCell>{item.source === "upstream" ? "上游同步" : "自定义"}</TableCell>
              <TableCell>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={!resource.ready || actions.isBusy(`remove-${item.model}`)}
                  onClick={() => remove(item.model)}
                >
                  移除
                </Button>
              </TableCell>
            </TableRow>
          ))}
          {!resource.data?.items.length && (
            <TableRow>
              <TableCell colSpan={3}>{resource.loading ? "加载中…" : "暂无可用模型"}</TableCell>
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
          <PaginationItem className="text-sm text-muted-foreground">
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
    </div>
  );
}
