"use client";
import { useState } from "react";
import {
  Plus,
  RefreshCw,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
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
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { Empty, EmptyDescription } from "@/components/ui/empty";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type SupplierTag, type List } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useTablePagination } from "@/lib/pagination";

export default function SupplierTagsPage() {
  const tags = useResource<List<SupplierTag>>("/supplier-tags");
  const actions = useActions();
  const [names, setNames] = useState<Record<string, string>>({});
  const [name, setName] = useState("");
  const [provider, setProvider] = useState("chatgpt");
  const pagination = useTablePagination(
    tags.data?.items ?? [],
    "supplier-tags",
    tags.data !== undefined,
  );
  useErrorToast(tags.error);
  return (
    <>
      <Card>
        <CardContent>
          <form
            noValidate
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) =>
              actions.submit(
                event,
                "create-supplier-tag",
                async () => {
                  if (!tags.ready) throw new Error("请先加载标签列表");
                  if (!name.trim()) throw new Error("请填写标签名称");
                  await request("/supplier-tags", {
                    method: "POST",
                    body: { name, provider_id: provider },
                  });
                  setName("");
                  tags.reload();
                },
                "标签已创建",
              )
            }
          >
            <Field className="w-44">
              <FieldLabel htmlFor="supplier-tag-platform">平台</FieldLabel>
              <Select value={provider} onValueChange={setProvider}>
                <SelectTrigger id="supplier-tag-platform">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="chatgpt">ChatGPT</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <Field className="w-60">
              <FieldLabel htmlFor="new-supplier-tag">标签名称</FieldLabel>
              <Input
                id="new-supplier-tag"
                value={name}
                onChange={(event) => setName(event.target.value)}
                maxLength={80}
              />
            </Field>
            <Button disabled={!tags.ready || actions.isBusy("create-supplier-tag")}>
              <Plus />
              创建标签
            </Button>
            <Button type="button" variant="ghost" onClick={tags.reload} disabled={tags.refreshing}>
              <RefreshCw />
              刷新
            </Button>
          </form>
        </CardContent>
      </Card>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>标签名称</TableHead>
                <TableHead>平台</TableHead>
                <TableHead>供应 / 虚拟账户</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {pagination.rows.map((tag) => (
                <TableRow key={tag.id}>
                  <TableCell>
                    <Input
                      aria-label={`标签名称 ${tag.name}`}
                      value={names[tag.id] ?? tag.name}
                      maxLength={80}
                      onChange={(event) => setNames({ ...names, [tag.id]: event.target.value })}
                    />
                  </TableCell>
                  <TableCell>
                    {tag.provider_id === "chatgpt" ? "ChatGPT" : tag.provider_id}
                  </TableCell>
                  <TableCell>
                    {tag.supplier_count} / {tag.binding_count}
                  </TableCell>
                  <TableCell>
                    <div className="flex gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={
                          !tags.ready ||
                          actions.isBusy(`tag-${tag.id}`) ||
                          names[tag.id] === undefined ||
                          names[tag.id] === tag.name
                        }
                        onClick={() =>
                          void actions.run(`tag-${tag.id}`, async () => {
                            if (!(names[tag.id] ?? "").trim()) throw new Error("请填写标签名称");
                            await request(`/supplier-tags/${tag.id}`, {
                              method: "PUT",
                              body: { name: names[tag.id], provider_id: tag.provider_id },
                            });
                            tags.reload();
                          })
                        }
                      >
                        保存
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={
                          !tags.ready || tag.binding_count > 0 || actions.isBusy(`tag-${tag.id}`)
                        }
                        title={
                          tag.binding_count > 0 ? "请先更换使用此号池的虚拟账户绑定" : undefined
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
              {tags.data?.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={4}>
                    <Empty>
                      <EmptyDescription>暂无标签</EmptyDescription>
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
                  <SelectTrigger aria-label="每页条数" className="w-24">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
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
                <Input className="w-14" {...pagination.input} />
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
    </>
  );
}
