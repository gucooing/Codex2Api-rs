"use client";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldLabel, FieldDescription } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from "@/components/ui/table";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type SupplierTag, type List } from "@/lib/api";
import { useResource } from "@/lib/hooks";

export function SupplierTags({ selected, onSaved }: { selected: string[]; onSaved: () => void }) {
  const [open, setOpen] = useState(false);
  const tags = useResource<List<SupplierTag>>(open ? "/supplier-tags" : null);
  const actions = useActions();
  const [names, setNames] = useState<Record<string, string>>({});
  const [name, setName] = useState("");
  const [checked, setChecked] = useState<string[]>([]);
  useErrorToast(tags.error);
  const refresh = () => {
    tags.reload();
    onSaved();
  };
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm">
          标签管理{selected.length ? `（已选 ${selected.length}）` : ""}
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>供应账户标签</DialogTitle>
          <DialogDescription>
            同一账户可加入多个标签号池。删除标签前须先调整引用它的虚拟账户。
          </DialogDescription>
        </DialogHeader>
        <form
          noValidate
          className="flex items-end gap-2"
          onSubmit={(event) =>
            actions.submit(event, "create-supplier-tag", async () => {
              if (!name.trim()) throw new Error("请填写标签名称");
              await request("/supplier-tags", {
                method: "POST",
                body: { name, provider_id: "chatgpt" },
              });
              setName("");
              refresh();
            })
          }
        >
          <Field>
            <FieldLabel htmlFor="new-supplier-tag">新标签名称</FieldLabel>
            <Input
              id="new-supplier-tag"
              value={name}
              onChange={(event) => setName(event.target.value)}
              maxLength={80}
            />
          </Field>
          <Button disabled={!tags.ready || actions.isBusy("create-supplier-tag")}>创建标签</Button>
          <Button type="button" variant="ghost" onClick={tags.reload}>
            刷新
          </Button>
        </form>
        <ScrollArea className="max-h-[50dvh]">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>选择</TableHead>
                <TableHead>名称</TableHead>
                <TableHead>供应 / 虚拟账户</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {(tags.data?.items ?? []).map((tag) => (
                <TableRow key={tag.id}>
                  <TableCell>
                    <Checkbox
                      aria-label={`选择标签 ${tag.name}`}
                      checked={checked.includes(tag.id)}
                      onCheckedChange={(value) =>
                        setChecked(
                          value ? [...checked, tag.id] : checked.filter((id) => id !== tag.id),
                        )
                      }
                    />
                  </TableCell>
                  <TableCell>
                    <Input
                      aria-label={`标签名称 ${tag.name}`}
                      value={names[tag.id] ?? tag.name}
                      maxLength={80}
                      onChange={(event) => setNames({ ...names, [tag.id]: event.target.value })}
                    />
                  </TableCell>
                  <TableCell>
                    {tag.supplier_count} / {tag.binding_count}
                  </TableCell>
                  <TableCell>
                    <div className="flex gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={!tags.ready || actions.isBusy(`tag-${tag.id}`)}
                        onClick={() =>
                          void actions.run(`tag-${tag.id}`, async () => {
                            await request(`/supplier-tags/${tag.id}`, {
                              method: "PUT",
                              body: {
                                name: names[tag.id] ?? tag.name,
                                provider_id: tag.provider_id,
                              },
                            });
                            refresh();
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
                        onClick={() =>
                          void actions.run(
                            `tag-${tag.id}`,
                            async () => {
                              await request(`/supplier-tags/${tag.id}`, { method: "DELETE" });
                              setChecked(checked.filter((id) => id !== tag.id));
                              refresh();
                            },
                            { confirm: "删除此标签？供应账户会保留。" },
                          )
                        }
                      >
                        删除
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </ScrollArea>
        <FieldDescription>
          已选择 {selected.length} 个供应账户、{checked.length} 个标签；批量操作保留其他标签。
        </FieldDescription>
        <div className="flex justify-end gap-2">
          {(["add", "remove"] as const).map((operation) => (
            <Button
              key={operation}
              variant={operation === "add" ? "default" : "outline"}
              disabled={
                !tags.ready || !selected.length || !checked.length || actions.isBusy("batch-tags")
              }
              onClick={() =>
                void actions.run("batch-tags", async () => {
                  await request("/suppliers/tags", {
                    method: "POST",
                    body: { account_ids: selected, tag_ids: checked, operation },
                  });
                  refresh();
                })
              }
            >
              {operation === "add" ? "批量添加标签" : "批量移除标签"}
            </Button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
