"use client";
import { useId, useState } from "react";
import {
  Plus,
  Search,
  RotateCcw,
  RefreshCw,
  Pencil,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from "@/components/ui/table";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { Empty, EmptyDescription } from "@/components/ui/empty";
import { Spinner } from "@/components/ui/spinner";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type SupplierTag, type List } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useTablePagination } from "@/lib/pagination";
import { useSavedFilters } from "@/lib/preferences";

export default function SupplierTagsPage() {
  const tags = useResource<List<SupplierTag>>("/supplier-tags");
  const actions = useActions();
  const id = useId();
  const empty = { search: "", provider_id: "" };
  const { filters, setFilters, applied, setApplied } = useSavedFilters(
    "supplier-tags.filters",
    empty,
  );
  const rows = (tags.data?.items ?? []).filter(
    (tag) =>
      (!applied.provider_id || tag.provider_id === applied.provider_id) &&
      tag.name.toLowerCase().includes(applied.search.trim().toLowerCase()),
  );
  const pagination = useTablePagination(rows, applied, tags.data !== undefined);
  const [editing, setEditing] = useState<SupplierTag | null>();
  const [name, setName] = useState("");
  const [provider, setProvider] = useState("chatgpt");
  const saving = actions.isBusy("supplier-tag-save");
  const openEditor = (tag?: SupplierTag) => {
    setName(tag?.name ?? "");
    setProvider(tag?.provider_id ?? (applied.provider_id || "chatgpt"));
    setEditing(tag ?? null);
  };
  useErrorToast(tags.error);
  return (
    <>
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters });
              tags.reload();
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
                  tags.reload();
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
                        onClick={() =>
                          void actions.run(
                            `tag-${tag.id}`,
                            async () => {
                              await request(`/supplier-tags/${tag.id}`, { method: "DELETE" });
                              tags.reload();
                            },
                            { confirm: `删除标签“${tag.name}”？供应账户会保留。` },
                          )
                        }
                      >
                        删除
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {tags.ready && !rows.length && (
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
      <Dialog
        open={editing !== undefined}
        onOpenChange={(open) => {
          if (!open && !saving) setEditing(undefined);
        }}
      >
        <DialogContent
          showCloseButton={!saving}
          onEscapeKeyDown={(event) => {
            if (saving) event.preventDefault();
          }}
          onInteractOutside={(event) => {
            if (saving) event.preventDefault();
          }}
        >
          <DialogHeader>
            <DialogTitle>{editing ? "编辑标签" : "添加标签"}</DialogTitle>
            <DialogDescription>设置标签名称和所属平台。</DialogDescription>
          </DialogHeader>
          <form
            noValidate
            className="space-y-4"
            onSubmit={(event) =>
              actions.submit(
                event,
                "supplier-tag-save",
                async () => {
                  if (editing === undefined || !tags.ready) throw new Error("请先加载标签资料");
                  if (!name.trim()) throw new Error("请填写标签名称");
                  await request(editing ? `/supplier-tags/${editing.id}` : "/supplier-tags", {
                    method: editing ? "PUT" : "POST",
                    body: { name: name.trim(), provider_id: provider },
                  });
                  setEditing(undefined);
                  tags.reload();
                },
                "标签已保存",
              )
            }
          >
            <FieldSet disabled={saving || !tags.ready}>
              <Field>
                <FieldLabel htmlFor={`${id}-name`}>标签名称</FieldLabel>
                <Input
                  id={`${id}-name`}
                  autoComplete="off"
                  maxLength={80}
                  required
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-edit-platform`}>平台</FieldLabel>
                <Select value={provider} disabled={!!editing} onValueChange={setProvider}>
                  <SelectTrigger id={`${id}-edit-platform`} className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="chatgpt">ChatGPT</SelectItem>
                    <SelectItem value="grok">Grok</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
            </FieldSet>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={saving}
                onClick={() => setEditing(undefined)}
              >
                取消
              </Button>
              <Button type="submit" disabled={saving || !tags.ready}>
                {saving && <Spinner />}保存
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}
