"use client";
import { defaults, useUsagePage } from "@/app/data/usage";
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
import { orderTime } from "@/lib/orders";
import { billingStatus, tokens, usageCost, usageStatus } from "@/lib/usage";

export default function UsagePage() {
  const {
    filters,
    setFilters,
    setApplied,
    setPage,
    id,
    resource,
    pagination,
    summary,
    update,
    summaries,
  } = useUsagePage();
  return (
    <>
      <Card>
        <CardContent>
          <form
            noValidate
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters, model: filters.model.trim() });
              setPage(1);
              resource.reload();
            }}
          >
            <Field className="w-32">
              <FieldLabel htmlFor={`${id}-days`}>时间范围</FieldLabel>
              <Select value={filters.days} onValueChange={(value) => update("days", value)}>
                <SelectTrigger id={`${id}-days`}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="1">今天</SelectItem>
                  <SelectItem value="7">最近 7 天</SelectItem>
                  <SelectItem value="30">最近 30 天</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <Field className="w-32">
              <FieldLabel htmlFor={`${id}-provider`}>平台</FieldLabel>
              <Select value={filters.provider} onValueChange={(value) => update("provider", value)}>
                <SelectTrigger id={`${id}-provider`}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="all">全部平台</SelectItem>
                  <SelectItem value="chatgpt">ChatGPT</SelectItem>
                  <SelectItem value="grok">Grok</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <Field className="w-48">
              <FieldLabel htmlFor={`${id}-model`}>模型</FieldLabel>
              <Input
                id={`${id}-model`}
                value={filters.model}
                maxLength={100}
                onChange={(e) => update("model", e.target.value)}
              />
            </Field>
            <Field className="w-32">
              <FieldLabel htmlFor={`${id}-status`}>结果</FieldLabel>
              <Select value={filters.status} onValueChange={(value) => update("status", value)}>
                <SelectTrigger id={`${id}-status`}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="all">全部结果</SelectItem>
                  <SelectItem value="completed">已完成</SelectItem>
                  <SelectItem value="failed">失败 / 中断</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <Button type="submit">查询 / 刷新</Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setFilters(defaults);
                setApplied(defaults);
                setPage(1);
                resource.reload();
              }}
            >
              重置
            </Button>
          </form>
        </CardContent>
      </Card>
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {summaries.map(([label, value]) => (
          <Card key={label}>
            <CardContent className="space-y-1">
              <p className="text-xs text-muted-foreground">{label}</p>
              <p className="text-lg font-semibold tabular-nums">{value}</p>
            </CardContent>
          </Card>
        ))}
      </div>
      <p className="text-xs text-muted-foreground">
        {summary
          ? `${summary.missing_token_requests} 次用量不完整 · ${summary.unpriced_requests} 次未计价`
          : "—"}
      </p>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>请求时间</TableHead>
                <TableHead>平台 / 模型</TableHead>
                <TableHead>结果</TableHead>
                <TableHead>输入</TableHead>
                <TableHead>输出</TableHead>
                <TableHead>缓存命中 / 写入</TableHead>
                <TableHead>推理</TableHead>
                <TableHead>计费用量（USD）</TableHead>
                <TableHead>计费状态</TableHead>
                <TableHead>首字节 / 总耗时</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {resource.data?.items.map((record) => (
                <TableRow key={record.id}>
                  <TableCell>{orderTime(record.requested_at_ms)}</TableCell>
                  <TableCell>
                    {record.provider_id === "chatgpt"
                      ? "ChatGPT"
                      : record.provider_id === "grok"
                        ? "Grok"
                        : record.provider_id}
                    <p className="text-xs text-muted-foreground">
                      {record.actual_model ?? record.model ?? "未记录模型"}
                    </p>
                  </TableCell>
                  <TableCell>
                    <Badge variant="secondary">{usageStatus(record.status)}</Badge>
                  </TableCell>
                  <TableCell>{tokens(record.input_tokens)}</TableCell>
                  <TableCell>{tokens(record.output_tokens)}</TableCell>
                  <TableCell>
                    {tokens(record.cached_tokens)} / {tokens(record.cache_write_tokens)}
                  </TableCell>
                  <TableCell>{tokens(record.reasoning_tokens)}</TableCell>
                  <TableCell>{usageCost(record.cost_nano_usd)}</TableCell>
                  <TableCell>{billingStatus(record.billing_status)}</TableCell>
                  <TableCell>
                    {record.first_byte_ms == null
                      ? "—"
                      : `${(record.first_byte_ms / 1000).toFixed(2)}s`}{" "}
                    / {record.total_ms == null ? "—" : `${(record.total_ms / 1000).toFixed(2)}s`}
                  </TableCell>
                </TableRow>
              ))}
              {resource.data?.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={10}>所选时间内暂无使用记录</TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end">
            <PaginationContent className="flex-wrap">
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
    </>
  );
}
