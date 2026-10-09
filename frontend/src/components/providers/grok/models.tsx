"use client";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { date } from "@/lib/format";
import { useGrokModelCatalog, useGrokModelSync } from "@/lib/providers/grok/data";
import Link from "next/link";

export function GrokModelSync({ onSynced }: { onSynced: () => void }) {
  const { actions, key, handleClick } = useGrokModelSync({ onSynced });
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={actions.isBusy(key)}
      onClick={() => handleClick()}
    >
      {actions.isBusy(key) && <Spinner />}同步 Grok 模型
    </Button>
  );
}

export function GrokModelCatalog({ id }: { id: string }) {
  const { resource, pagination, actions, handleClick } = useGrokModelCatalog({ id });
  return (
    <Card>
      <CardContent className="space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="outline"
            disabled={actions.isBusy("grok-catalog")}
            onClick={() => handleClick()}
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
