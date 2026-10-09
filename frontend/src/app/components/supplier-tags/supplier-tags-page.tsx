"use client";
import { SupplierTagEditorDialog } from "@/app/components/supplier-tags/supplier-tag-editor-dialog";
import { useSupplierTagsPage } from "@/app/data/supplier-tags";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
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
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
} from "lucide-react";

export default function SupplierTagsPage() {
  const {
    actions,
    id,
    empty,
    filters,
    setFilters,
    setApplied,
    tags,
    pagination,
    editing,
    setEditing,
    name,
    setName,
    provider,
    setProvider,
    saving,
    openEditor,
    handleClick,
  } = useSupplierTagsPage();
  return (
    <>
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters });
              tags.reload(1);
            }}
          >
            <Field className="w-40">
              <FieldLabel htmlFor={`${id}-search`}>搜索标签</FieldLabel>
              <Input
                id={`${id}-search`}
                value={filters.search}
                placeholder="标签名称"
                onChange={(event) => setFilters({ ...filters, search: event.target.value })}
              />
            </Field>
            <Field className="w-40">
              <FieldLabel htmlFor={`${id}-platform`}>平台</FieldLabel>
              <Select
                value={filters.provider_id || "all"}
                onValueChange={(value) =>
                  setFilters({ ...filters, provider_id: value === "all" ? "" : value })
                }
              >
                <SelectTrigger id={`${id}-platform`} className="w-full">
                  <SelectValue placeholder="全部平台" />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="all">全部平台</SelectItem>
                  <SelectItem value="chatgpt">ChatGPT</SelectItem>
                  <SelectItem value="grok">Grok</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <div className="flex items-center gap-2">
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
                  tags.reload(1);
                }}
              >
                <RotateCcw />
                重置
              </Button>
            </div>
          </form>
          <div className="ml-auto flex items-center gap-2">
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label="刷新标签"
              title="刷新"
              onClick={tags.reload}
              disabled={tags.refreshing}
            >
              <RefreshCw />
            </Button>
            <Button type="button" disabled={!tags.ready} onClick={() => openEditor()}>
              <Plus />
              添加标签
            </Button>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>标签</TableHead>
                <TableHead>平台</TableHead>
                <TableHead>供应 / 虚拟账户</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {pagination.rows.map((tag) => (
                <TableRow key={tag.id}>
                  <TableCell>{tag.name}</TableCell>
                  <TableCell>
                    {tag.provider_id === "chatgpt"
                      ? "ChatGPT"
                      : tag.provider_id === "grok"
                        ? "Grok"
                        : tag.provider_id}
                  </TableCell>
                  <TableCell>
                    {tag.supplier_count} / {tag.binding_count}
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!tags.ready}
                        onClick={() => openEditor(tag)}
                      >
                        <Pencil />
                        编辑
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={
                          !tags.ready || tag.binding_count > 0 || actions.isBusy(`tag-${tag.id}`)
                        }
                        title={
                          tag.binding_count > 0 ? "请先更换使用此标签的虚拟账户绑定" : undefined
                        }
                        onClick={() => handleClick(tag)}
                      >
                        删除
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {tags.ready && !pagination.rows.length && (
                <TableRow>
                  <TableCell colSpan={4}>
                    <Empty>
                      <EmptyDescription>暂无匹配标签</EmptyDescription>
                    </Empty>
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="标签分页">
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
        </CardContent>
      </Card>
      <SupplierTagEditorDialog
        editing={editing}
        saving={saving}
        setEditing={setEditing}
        actions={actions}
        tags={tags}
        name={name}
        provider={provider}
        id={id}
        setName={setName}
        setProvider={setProvider}
      />
    </>
  );
}
