"use client";
import { ApiKeyEditorDialog } from "@/app/components/api-keys/api-key-editor-dialog";
import { ApiKeyRevokeDialog } from "@/app/components/api-keys/api-key-revoke-dialog";
import { ApiKeySecretDialog } from "@/app/components/api-keys/api-key-secret-dialog";
import { useApiKeysPage } from "@/app/data/api-keys";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
import { apiCost, apiDate, apiKeyStatus } from "@/lib/api-access";
import { tokens } from "@/lib/usage";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Copy,
  MoreHorizontal,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
} from "lucide-react";

export default function ApiKeysPage() {
  const state = useApiKeysPage();
  const {
    id,
    groups,
    resource,
    pagination,
    filters,
    setFilters,
    search,
    reset,
    reload,
    openEditor,
    baseRef,
    copyBase,
    setRevoking,
  } = state;
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm text-muted-foreground">API 基础地址</span>
        <p ref={baseRef} className="min-w-0 select-all break-all font-mono text-sm">
          {groups.data?.api_base_url ?? "—"}
        </p>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label="复制 API 基础地址"
          disabled={!groups.ready}
          onClick={copyBase}
        >
          <Copy />
        </Button>
        <Badge variant="outline">USD 钱包按量计费</Badge>
      </div>
      <form className="grid grid-cols-2 items-center gap-2 sm:flex sm:flex-wrap" onSubmit={search}>
        <Field className="min-w-0 sm:w-40">
          <FieldLabel className="sr-only" htmlFor={id + "-search"}>
            密钥名称
          </FieldLabel>
          <Input
            id={id + "-search"}
            placeholder="搜索密钥名称"
            value={filters.search}
            onChange={(event) => setFilters({ ...filters, search: event.target.value })}
          />
        </Field>
        <Field className="min-w-0 sm:w-40">
          <FieldLabel className="sr-only" htmlFor={id + "-group"}>
            API 组
          </FieldLabel>
          <Select
            value={filters.group_id || "all"}
            onValueChange={(group_id) =>
              setFilters({ ...filters, group_id: group_id === "all" ? "" : group_id })
            }
          >
            <SelectTrigger id={id + "-group"} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="all">全部 API 组</SelectItem>
              {groups.data?.items.map((group) => (
                <SelectItem key={group.id} value={group.id}>
                  {group.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field className="min-w-0 sm:w-28">
          <FieldLabel className="sr-only" htmlFor={id + "-status"}>
            状态
          </FieldLabel>
          <Select
            value={filters.status || "all"}
            onValueChange={(status) =>
              setFilters({ ...filters, status: status === "all" ? "" : status })
            }
          >
            <SelectTrigger id={id + "-status"} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="all">全部状态</SelectItem>
              <SelectItem value="enabled">已启用</SelectItem>
              <SelectItem value="disabled">已停用</SelectItem>
              <SelectItem value="revoked">已撤销</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <div className="flex flex-wrap items-center gap-1.5">
          <Button type="submit" size="sm">
            <Search />
            查询
          </Button>
          <Button type="button" variant="secondary" size="sm" onClick={reset}>
            <RotateCcw />
            重置
          </Button>
        </div>
        <div className="col-span-2 flex items-center justify-end gap-2 sm:col-auto sm:ml-auto">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="刷新密钥"
            onClick={reload}
          >
            <RefreshCw />
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={!resource.ready || !groups.ready || !groups.data?.items.length}
            onClick={() => openEditor()}
          >
            <Plus />
            创建密钥
          </Button>
        </div>
      </form>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>密钥 / API 组</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>累计用量</TableHead>
                <TableHead>当前并发</TableHead>
                <TableHead className="max-md:hidden">最后使用</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {resource.data?.items.map((key) => (
                <TableRow key={key.id}>
                  <TableCell>
                    <p className="max-w-44 truncate font-medium">{key.name}</p>
                    <p className="text-xs text-muted-foreground">{key.key_prefix}…</p>
                    <p className="text-xs text-muted-foreground">{key.group_name}</p>
                  </TableCell>
                  <TableCell>
                    <Badge variant={key.enabled && !key.revoked_at ? "secondary" : "outline"}>
                      {apiKeyStatus(key)}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <p>{apiCost(key.cost_nano_usd)}</p>
                    <p className="text-xs text-muted-foreground">
                      {key.total_requests} 次 · 输入 {tokens(key.input_tokens)} / 输出{" "}
                      {tokens(key.output_tokens)}
                    </p>
                    {key.unknown_cost_requests > 0 && (
                      <p className="text-xs text-muted-foreground">
                        {key.unknown_cost_requests} 次费用未知
                      </p>
                    )}
                  </TableCell>
                  <TableCell>{key.active_requests}</TableCell>
                  <TableCell className="max-md:hidden">{apiDate(key.last_used_at)}</TableCell>
                  <TableCell>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          aria-label={"操作 " + key.name}
                        >
                          <MoreHorizontal />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem
                          disabled={!resource.ready || !!key.revoked_at}
                          onSelect={() => openEditor(key)}
                        >
                          编辑
                        </DropdownMenuItem>
                        <DropdownMenuItem
                          variant="destructive"
                          disabled={!resource.ready || !!key.revoked_at}
                          onSelect={() => setRevoking(key)}
                        >
                          撤销
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              ))}
              {resource.ready && !resource.data?.items.length && (
                <TableRow>
                  <TableCell colSpan={6} className="text-center text-muted-foreground">
                    暂无 API 密钥
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="API 密钥分页">
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
      <ApiKeyEditorDialog state={state} />
      <ApiKeySecretDialog state={state} />
      <ApiKeyRevokeDialog state={state} />
    </>
  );
}
