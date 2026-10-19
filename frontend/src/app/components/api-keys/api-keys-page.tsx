"use client";
import { ApiKeyDetailDialog } from "@/app/components/api-keys/api-key-detail-dialog";
import { useApiKeysPage } from "@/app/data/api-keys";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
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
import { date } from "@/lib/format";
import { apiCost, apiKeyStatus, apiTokens } from "@/lib/api-access";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  RefreshCw,
  RotateCcw,
  Search,
} from "lucide-react";

export default function ApiKeysPage() {
  const {
    id,
    filters,
    setFilters,
    groups,
    resource,
    pagination,
    columns,
    selected,
    setSelected,
    search,
    reset,
  } = useApiKeysPage();
  return (
    <>
      <Card size="sm">
        <CardContent>
          <form
            className="grid w-full grid-cols-2 items-center gap-2 sm:flex sm:flex-wrap"
            onSubmit={search}
          >
            <Field className="min-w-0 sm:w-44">
              <FieldLabel className="sr-only" htmlFor={id + "-search"}>
                用户 / 密钥
              </FieldLabel>
              <Input
                id={id + "-search"}
                placeholder="搜索用户名或密钥名称"
                value={filters.search}
                onChange={(event) => setFilters({ ...filters, search: event.target.value })}
              />
            </Field>
            <Field className="min-w-0 sm:w-36">
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
              <Button type="button" variant="outline" size="sm" onClick={reset}>
                <RotateCcw />
                重置
              </Button>
            </div>
            <div className="col-span-2 flex items-center justify-end gap-1.5 sm:col-auto sm:ml-auto">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button type="button" variant="outline" size="sm">
                    <Columns3 />
                    显示列
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuLabel>
                    {columns.mobile ? "手机显示列" : "桌面显示列"}
                  </DropdownMenuLabel>
                  {columns.labels.map((label) => (
                    <DropdownMenuCheckboxItem
                      key={label}
                      checked={columns.isVisible(label)}
                      disabled={columns.count === 1 && columns.isVisible(label)}
                      onSelect={(event) => event.preventDefault()}
                      onCheckedChange={(checked) => columns.setVisible(label, checked === true)}
                    >
                      {label}
                    </DropdownMenuCheckboxItem>
                  ))}
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onSelect={columns.showAll}>显示全部列</DropdownMenuItem>
                  <DropdownMenuItem onSelect={columns.reset}>恢复默认列</DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="刷新 API 密钥"
                disabled={resource.refreshing}
                onClick={resource.reload}
              >
                <RefreshCw />
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                {columns.labels.map((label) => (
                  <TableHead key={label} hidden={!columns.isVisible(label)}>
                    {label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {pagination.rows.map((key) => (
                <TableRow key={key.id}>
                  <TableCell hidden={!columns.isVisible("用户 / 密钥")}>
                    <p className="max-w-48 truncate font-medium">
                      {key.username} / {key.name}
                    </p>
                    <p className="font-mono text-xs text-muted-foreground">{key.key_prefix}…</p>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("API 组")}>{key.group_name}</TableCell>
                  <TableCell hidden={!columns.isVisible("状态")}>
                    <Badge variant={key.enabled && !key.revoked_at ? "secondary" : "outline"}>
                      {apiKeyStatus(key)}
                    </Badge>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("请求数")} className="tabular-nums">
                    {key.total_requests}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("输入 / 输出")} className="tabular-nums">
                    {apiTokens(key.input_tokens)} / {apiTokens(key.output_tokens)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("计费金额")} className="tabular-nums">
                    {apiCost(key.cost_nano_usd)}
                    {key.unknown_cost_requests > 0 && (
                      <p className="text-xs text-muted-foreground">
                        {key.unknown_cost_requests} 次费用未知
                      </p>
                    )}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("当前并发")} className="tabular-nums">
                    {key.active_requests}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("最后使用")}>
                    {date(key.last_used_at)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作")}>
                    <Button variant="outline" size="sm" onClick={() => setSelected(key)}>
                      详情
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {resource.ready && !pagination.rows.length && (
                <TableRow>
                  <TableCell colSpan={columns.count} className="text-center text-muted-foreground">
                    暂无 API 密钥
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="密钥分页">
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
      <ApiKeyDetailDialog selected={selected} onClose={() => setSelected(undefined)} />
    </>
  );
}
