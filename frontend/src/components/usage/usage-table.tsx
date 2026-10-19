"use client";
import { Button } from "@/components/ui/button";
import { CardDescription } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Empty, EmptyDescription } from "@/components/ui/empty";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { UsageDetailDialog } from "@/components/usage/usage-detail-dialog";
import { type UsageRecord } from "@/lib/api";
import { date } from "@/lib/format";
import { UsageFee } from "./usage-fee";
import { useUsageTable } from "@/lib/usage";
import {
  cacheRate,
  duration,
  imageUsageLabel,
  isTextUsage,
  requestSpeed,
  tokenCount,
  usageResult,
  usageStatus,
  usageType,
  usageSource,
} from "@/lib/usage-display";
import { ChevronRight, Columns3, Info } from "lucide-react";

export function UsageTable({ records, empty }: { records: UsageRecord[]; empty?: string }) {
  const { tableColumns0, mobile, selected, setSelected } = useUsageTable();
  return (
    <>
      <>
        <div className="mb-2 flex justify-end">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" variant="outline" size="sm" aria-label="显示列">
                <Columns3 />
                显示列
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <DropdownMenuLabel>
                {tableColumns0.mobile ? "手机显示列" : "桌面显示列"}
              </DropdownMenuLabel>
              {tableColumns0.labels.map((label) => (
                <DropdownMenuCheckboxItem
                  key={label}
                  checked={tableColumns0.isVisible(label)}
                  disabled={tableColumns0.count === 1 && tableColumns0.isVisible(label)}
                  onSelect={(event) => event.preventDefault()}
                  onCheckedChange={(checked) => tableColumns0.setVisible(label, checked === true)}
                >
                  {label}
                </DropdownMenuCheckboxItem>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={tableColumns0.showAll}>显示全部列</DropdownMenuItem>
              <DropdownMenuItem onSelect={tableColumns0.reset}>恢复默认列</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        <Table
          role="table"
          className={
            tableColumns0.count > 4
              ? "[&_td]:py-1.5 max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
              : "[&_td]:py-1.5 max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
          }
        >
          <TableHeader>
            <TableRow role="row">
              {[
                "消费账户",
                "供应账户",
                "来源",
                "类型",
                "模型",
                "接口",
                "IP",
                "推理强度 / 速度",
                "用量",
                "费用",
                "耗时",
                "时间 / 状态",
              ].map((label) => (
                <TableHead
                  hidden={!tableColumns0.isVisible(label)}
                  className={
                    ["模型", "用量", "时间 / 状态"].includes(label)
                      ? label === "模型"
                        ? ""
                        : "max-md:w-20"
                      : ""
                  }
                  key={label}
                >
                  {label}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {records.length ? (
              records.map((record) => {
                const mismatch = Boolean(
                  record.actual_model && record.model && record.actual_model !== record.model,
                );
                const metrics = [
                  {
                    label: "输入",
                    value: tokenCount(record.input_tokens),
                    exact: record.input_tokens,
                  },
                  {
                    label: "输出",
                    value: tokenCount(record.output_tokens),
                    exact: record.output_tokens,
                  },
                  {
                    label: "思考",
                    description: "思考（包含在输出中）",
                    value: tokenCount(record.reasoning_tokens),
                    exact: record.reasoning_tokens,
                  },
                  {
                    label: "缓存读取",
                    value: tokenCount(record.cached_tokens),
                    exact: record.cached_tokens,
                  },
                  {
                    label: "缓存写入",
                    value: tokenCount(record.cache_write_tokens),
                    exact: record.cache_write_tokens,
                  },
                  {
                    label: "缓存率",
                    description: "缓存率（缓存读取 / 总输入）",
                    value: cacheRate(record),
                    exact: undefined,
                  },
                ];
                return (
                  <TableRow role="row" key={record.id}>
                    <TableCell
                      hidden={!tableColumns0.isVisible("消费账户")}
                      className=" "
                      data-label="消费账户"
                      role="cell"
                    >
                      {record.consumer_name || "—"}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("供应账户")}
                      className=" "
                      data-label="供应账户"
                      role="cell"
                    >
                      {record.account_name}
                      <CardDescription>{record.provider_id}</CardDescription>
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("来源")}
                      data-label="来源"
                      role="cell"
                      className="whitespace-nowrap text-xs"
                    >
                      {usageSource(record.subject_kind)}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("类型")}
                      data-label="类型"
                      role="cell"
                      className="whitespace-nowrap text-xs"
                    >
                      {usageType(record.stream, record.transport)}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("模型")}
                      className=" max-md:overflow-hidden"
                      data-label="模型"
                      role="cell"
                    >
                      {mobile ? (
                        <Button
                          type="button"
                          variant="ghost"
                          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
                          aria-label={
                            "查看详情：" +
                            String(record.actual_model ?? record.model ?? "未记录模型")
                          }
                          onClick={() => setSelected(record)}
                        >
                          <span className="min-w-0 flex-1">
                            <span className="block truncate font-medium">
                              {record.actual_model ?? record.model ?? "未记录模型"}
                            </span>
                            <span className="block truncate text-xs text-muted-foreground">
                              {new Date(record.requested_at_ms).toLocaleTimeString("zh-CN", {
                                hour: "2-digit",
                                minute: "2-digit",
                              }) +
                                " · " +
                                (record.consumer_name || "—")}
                            </span>
                          </span>
                          <ChevronRight className="size-3 shrink-0" />
                        </Button>
                      ) : (
                        <div className="max-md:hidden">
                          <strong className={mismatch ? "text-foreground" : undefined}>
                            {mismatch
                              ? `${record.model} → ${record.actual_model}`
                              : (record.actual_model ?? record.model ?? "-")}
                          </strong>
                        </div>
                      )}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("接口")}
                      data-label="接口"
                      role="cell"
                      className="whitespace-nowrap text-xs"
                    >
                      <div>
                        <strong>入站:</strong> {record.endpoint || "—"}
                      </div>
                      <div>
                        <strong>上游:</strong> {record.upstream_endpoint ?? "—"}
                      </div>
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("IP")}
                      data-label="IP"
                      role="cell"
                      className="whitespace-nowrap text-xs"
                    >
                      {record.client_ip ?? "—"}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("推理强度 / 速度")}
                      data-label="推理强度 / 速度"
                      data-compact="true"
                      role="cell"
                      className="text-xs "
                    >
                      {record.reasoning_effort ?? "默认"}
                      {isTextUsage(record) ? ` / ${requestSpeed(record)}` : ""}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("用量")}
                      className=" max-md:overflow-hidden"
                      data-label="用量"
                      role="cell"
                    >
                      {mobile ? (
                        <div className="space-y-0.5 text-xs tabular-nums md:hidden">
                          <div>
                            {record.image_count !== null
                              ? record.image_count + " 张"
                              : (record.input_tokens == null && record.output_tokens == null
                                  ? "-"
                                  : tokenCount(
                                      (record.input_tokens ?? 0) + (record.output_tokens ?? 0),
                                    )) + " Token"}
                          </div>
                          <div className="text-muted-foreground">
                            <UsageFee record={record} />
                          </div>
                        </div>
                      ) : (
                        <div className="max-md:hidden">
                          {record.image_count !== null ? (
                            <span>{imageUsageLabel(record)}</span>
                          ) : (
                            <div className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-xs tabular-nums md:grid-cols-[repeat(3,max-content)]">
                              {metrics.map(({ label, description, value, exact }) => (
                                <Tooltip key={label}>
                                  <TooltipTrigger asChild>
                                    <span
                                      tabIndex={0}
                                      className="inline-flex items-center gap-1 whitespace-nowrap"
                                      aria-label={`${label}：${value}`}
                                    >
                                      <span className="text-muted-foreground">{label}</span>
                                      {value}
                                    </span>
                                  </TooltipTrigger>
                                  <TooltipContent>
                                    {description ?? label}：
                                    {exact == null ? value : exact.toLocaleString("en-US")}
                                  </TooltipContent>
                                </Tooltip>
                              ))}
                            </div>
                          )}
                        </div>
                      )}
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("费用")}
                      data-label="费用"
                      data-compact="true"
                      role="cell"
                      className="tabular-nums "
                    >
                      <UsageFee record={record} />
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("耗时")}
                      className=" "
                      data-label="耗时"
                      role="cell"
                    >
                      <CardDescription className="text-xs">
                        首 Token {duration(record.first_token_ms)}
                      </CardDescription>
                      <CardDescription className="text-xs">
                        总计 {duration(record.total_ms)}
                      </CardDescription>
                    </TableCell>
                    <TableCell
                      hidden={!tableColumns0.isVisible("时间 / 状态")}
                      className=" max-md:overflow-hidden"
                      data-label="时间 / 状态"
                      role="cell"
                    >
                      <span className="hidden text-xs md:inline">
                        {date(record.requested_at_ms)}
                      </span>
                      <div>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          className="h-auto min-h-6 max-w-full flex-wrap justify-start px-1 text-left tabular-nums"
                          aria-label={`查看请求详情：${usageResult(record)}`}
                          onClick={() => setSelected(record)}
                        >
                          <span className={usageStatus(record.status).className}>
                            {usageResult(record)}
                          </span>
                          <Info className="size-3" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })
            ) : (
              <TableRow role="row">
                <TableCell role="cell" colSpan={tableColumns0.count}>
                  <Empty>
                    <EmptyDescription>{empty ?? "暂无符合条件的用量记录"}</EmptyDescription>
                  </Empty>
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </>
      <UsageDetailDialog selected={selected} setSelected={setSelected} />
    </>
  );
}
