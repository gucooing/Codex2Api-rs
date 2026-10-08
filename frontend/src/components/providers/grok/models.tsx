"use client";
import Link from "next/link";
import { toast } from "sonner";
import { request, type List, type Json } from "@/lib/api";
import { useListResource } from "@/lib/pagination";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useActions, useErrorToast } from "@/lib/actions";
import { date } from "@/lib/format";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableHeader,
  TableBody,
  TableHead,
  TableRow,
  TableCell,
} from "@/components/ui/table";
import { Card, CardContent, CardDescription } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";

export function GrokModelSync({ onSynced }: { onSynced: () => void }) {
  const actions = useActions();
  const key = "grok-model-sync";
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={actions.isBusy(key)}
      onClick={() =>
        void actions.run(
          key,
          async () => {
            const result = await request<List<{ success: boolean; models?: number }>>(
              "/models/grok/sync",
              { method: "POST" },
            );
            onSynced();
            const failed = result.items.filter((i) => !i.success).length;
            if (failed) toast.error(`${failed} 个 Grok 账户同步失败，其余目录已更新`);
            else toast.success("Grok 真实模型目录与可用价格预设已同步");
          },
          { success: "" },
        )
      }
    >
      {actions.isBusy(key) && <Spinner />}同步 Grok 模型
    </Button>
  );
}
type GrokModel = {
  id: string;
  model: string;
  name?: string;
  context_window?: number;
  api_backend: string;
};
export function GrokModelCatalog({ id }: { id: string }) {
  const resource = useListResource<GrokModel, { observed_at: string | null; stale: boolean }>(
    `/suppliers/grok/${encodeURIComponent(id)}/models`,
  );
  const pagination = resource.pagination;
  const actions = useActions();
  useErrorToast(resource.error);
  return (
    <Card>
      <CardContent className="space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="outline"
            disabled={actions.isBusy("grok-catalog")}
            onClick={() =>
              void actions.run("grok-catalog", async () => {
                await request<Json>(`/suppliers/grok/${encodeURIComponent(id)}/models`, {
                  method: "POST",
                });
                resource.reload();
              })
            }
          >
            从 Grok Build 同步
          </Button>
          <Button variant="outline" asChild>
            <Link href="/models/">管理模型计价</Link>
          </Button>
          <CardDescription>采集时间：{date(resource.data?.observed_at)}</CardDescription>
        </div>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>模型</TableHead>
              <TableHead>名称</TableHead>
              <TableHead>上下文</TableHead>
              <TableHead>接口</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {(resource.data?.items ?? []).map((model) => (
              <TableRow key={model.model}>
                <TableCell>{model.model}</TableCell>
                <TableCell>{model.name ?? "—"}</TableCell>
                <TableCell>{model.context_window ?? "—"}</TableCell>
                <TableCell>{model.api_backend}</TableCell>
              </TableRow>
            ))}
            {!resource.data?.items.length && (
              <TableRow>
                <TableCell colSpan={4}>
                  {resource.loading
                    ? "正在读取目录…"
                    : resource.data?.observed_at
                      ? "该账户未返回可用模型"
                      : "尚未同步官方模型目录"}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
        <Pagination>
          <PaginationContent>
            <PaginationItem>
              共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
            </PaginationItem>
            <PaginationItem>
              <Select {...pagination.size}>
                <SelectTrigger aria-label="每页条数">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[10, 20, 30, 50].map((n) => (
                    <SelectItem key={n} value={String(n)}>
                      {n} 条
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </PaginationItem>
            <PaginationItem>
              <Button variant="outline" {...pagination.first}>
                首页
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Button variant="outline" {...pagination.previous}>
                上一页
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Input className="w-16" {...pagination.input} />
            </PaginationItem>
            <PaginationItem>
              <Button variant="outline" {...pagination.next}>
                下一页
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Button variant="outline" {...pagination.last}>
                末页
              </Button>
            </PaginationItem>
          </PaginationContent>
        </Pagination>
      </CardContent>
    </Card>
  );
}
