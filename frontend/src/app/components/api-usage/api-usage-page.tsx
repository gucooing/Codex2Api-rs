"use client";
import { ApiUsageDetailDialog } from "@/app/components/api-usage/api-usage-detail-dialog";
import { useApiUsagePage } from "@/app/data/api-usage";
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
import { apiCost, apiTokens, apiUsageStatus } from "@/lib/metered-api";
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
export default function ApiUsagePage() {
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
  } = useApiUsagePage();
  const summary = resource.data?.summary;
  return (
    <>
      <form className="flex flex-wrap items-end gap-3" noValidate onSubmit={search}>
        <Field className="w-44">
          <FieldLabel htmlFor={id + "-search"}>用户 / 模型 / 密钥</FieldLabel>
          <Input
            id={id + "-search"}
            value={filters.search}
            onChange={(event) => setFilters({ ...filters, search: event.target.value })}
          />
        </Field>
        <Field className="w-40">
          <FieldLabel htmlFor={id + "-group"}>API 组</FieldLabel>
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
        <Field className="w-32">
          <FieldLabel htmlFor={id + "-status"}>状态</FieldLabel>
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
              <SelectItem value="in_progress">进行中</SelectItem>
              <SelectItem value="completed">已完成</SelectItem>
              <SelectItem value="failed">失败</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Field className="w-48">
          <FieldLabel htmlFor={id + "-from"}>开始时间</FieldLabel>
          <Input
            id={id + "-from"}
            type="datetime-local"
            value={filters.from}
            onChange={(event) => setFilters({ ...filters, from: event.target.value })}
          />
        </Field>
        <Field className="w-48">
          <FieldLabel htmlFor={id + "-until"}>结束时间</FieldLabel>
          <Input
            id={id + "-until"}
            type="datetime-local"
            value={filters.until}
            onChange={(event) => setFilters({ ...filters, until: event.target.value })}
          />
        </Field>
        <Button type="submit">
          <Search />
          查询
        </Button>
        <Button type="button" variant="secondary" onClick={reset}>
          <RotateCcw />
          重置
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button type="button" variant="outline">
              <Columns3 />
              显示列
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel>{columns.mobile ? "手机显示列" : "桌面显示列"}</DropdownMenuLabel>
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
          className="ml-auto"
          variant="ghost"
          size="icon-sm"
          aria-label="刷新 API 用量"
          disabled={resource.refreshing}
          onClick={resource.reload}
        >
          <RefreshCw />
        </Button>
      </form>
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Card>
          <CardContent>
            <p className="text-xs text-muted-foreground">请求数</p>
            <p className="text-lg tabular-nums">{summary?.requests ?? "—"}</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs text-muted-foreground">输入 / 输出 Token</p>
            <p className="text-lg tabular-nums">
              {summary ? apiTokens(summary.input_tokens) : "—"} /{" "}
              {summary ? apiTokens(summary.output_tokens) : "—"}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs text-muted-foreground">计费金额</p>
            <p className="text-lg tabular-nums">{summary ? apiCost(summary.cost_nano_usd) : "—"}</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs text-muted-foreground">费用未知请求</p>
            <p className="text-lg tabular-nums">{summary?.unknown_cost_requests ?? "—"}</p>
          </CardContent>
        </Card>
      </div>
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
              {pagination.rows.map((record) => (
                <TableRow key={record.id}>
                  <TableCell hidden={!columns.isVisible("时间")}>
                    {date(record.requested_at_ms)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("用户 / 密钥")}>
                    <p>
                      {record.username} / {record.key_name}
                    </p>
                    <p className="text-xs text-muted-foreground">{record.key_prefix}…</p>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("API 组")}>{record.group_name}</TableCell>
                  <TableCell hidden={!columns.isVisible("模型 / 接口")}>
                    <p className="max-w-56 truncate" title={record.model}>
                      {record.actual_model && record.actual_model !== record.model
                        ? record.model + " → " + record.actual_model
                        : record.model}
                    </p>
                    <p className="text-xs text-muted-foreground">{record.endpoint}</p>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("状态")}>
                    <Badge variant="outline">
                      {apiUsageStatus(record.status)}
                      {record.http_status ? " " + record.http_status : ""}
                    </Badge>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("输入 / 输出")}>
                    {apiTokens(record.input_tokens)} / {apiTokens(record.output_tokens)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("倍率 / 费用")}>
                    <p>{apiCost(record.cost_nano_usd)}</p>
                    <p className="text-xs text-muted-foreground">{record.multiplier}×</p>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作")}>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => setSelected(record)}
                    >
                      详情
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {resource.ready && !pagination.rows.length && (
                <TableRow>
                  <TableCell colSpan={columns.count} className="text-center text-muted-foreground">
                    暂无 API 调用记录
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="API 用量分页">
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
      <ApiUsageDetailDialog selected={selected} onClose={() => setSelected(undefined)} />
    </>
  );
}
