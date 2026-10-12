"use client";
import { ApiUsageDetailDialog } from "@/app/components/api-usage/api-usage-detail-dialog";
import { useApiUsagePage } from "@/app/data/api-usage";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
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
import { apiCost, apiDate } from "@/lib/metered-api";
import { tokens, usageStatus } from "@/lib/usage";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
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
          <FieldLabel htmlFor={id + "-search"}>模型 / 密钥</FieldLabel>
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
        <Button
          type="button"
          className="ml-auto"
          variant="ghost"
          size="icon-sm"
          aria-label="刷新 API 用量"
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
              {tokens(summary?.input_tokens)} / {tokens(summary?.output_tokens)}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs text-muted-foreground">计费金额</p>
            <p className="text-lg tabular-nums">{apiCost(summary?.cost_nano_usd)}</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs text-muted-foreground">费用未知请求</p>
            <p className="text-lg tabular-nums">{summary?.unknown_cost_requests ?? "—"}</p>
          </CardContent>
        </Card>
      </div>
      <p className="text-xs text-muted-foreground">
        累计待扣费用：{apiCost(resource.data?.wallet_pending_nano_usd)}
      </p>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>模型 / 接口</TableHead>
                <TableHead className="max-md:hidden">密钥 / API 组</TableHead>
                <TableHead>状态</TableHead>
                <TableHead className="max-md:hidden">输入 / 输出</TableHead>
                <TableHead>倍率 / 费用</TableHead>
                <TableHead className="max-md:hidden">时间</TableHead>
                <TableHead>详情</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {resource.data?.items.map((record) => (
                <TableRow key={record.id}>
                  <TableCell>
                    <p className="max-w-44 truncate" title={record.model}>
                      {record.actual_model && record.actual_model !== record.model
                        ? record.model + " → " + record.actual_model
                        : record.model}
                    </p>
                    <p className="text-xs text-muted-foreground">{record.endpoint}</p>
                  </TableCell>
                  <TableCell className="max-md:hidden">
                    {record.key_name}
                    <p className="text-xs text-muted-foreground">{record.group_name}</p>
                  </TableCell>
                  <TableCell>
                    <Badge variant="outline">
                      {usageStatus(record.status)}
                      {record.http_status ? " " + record.http_status : ""}
                    </Badge>
                  </TableCell>
                  <TableCell className="max-md:hidden">
                    {tokens(record.input_tokens)} / {tokens(record.output_tokens)}
                  </TableCell>
                  <TableCell>
                    {apiCost(record.cost_nano_usd)}
                    <p className="text-xs text-muted-foreground">{record.multiplier}×</p>
                  </TableCell>
                  <TableCell className="max-md:hidden">{apiDate(record.requested_at_ms)}</TableCell>
                  <TableCell>
                    <Button variant="ghost" size="sm" onClick={() => setSelected(record)}>
                      查看
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {resource.ready && !resource.data?.items.length && (
                <TableRow>
                  <TableCell colSpan={7} className="text-center text-muted-foreground">
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
