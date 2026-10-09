"use client";
import { RecordDialog } from "@/components/platform-account/record-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
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
import { useLogs } from "@/lib/platform-account";
import { mobileRecordColumns, recordColumns, recordRows } from "@/lib/records";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  RotateCcw,
  Search,
} from "lucide-react";

export function Logs({ id }: { id: string }) {
  const {
    tableColumns5,
    tableColumns6,
    tableColumns7,
    fieldId,
    setPage,
    empty,
    filters,
    setFilters,
    setApplied,
    resource,
    rows,
    pagination,
    analyticsResource,
    sitesResource,
    analytics,
    sites,
  } = useLogs({ id });
  return (
    <>
      <Card size="sm">
        <CardContent className="space-y-4">
          <form
            className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters });
              setPage(1);
              resource.reload();
            }}
          >
            <Field>
              <FieldLabel
                htmlFor={fieldId + "-field-19" + "-" + encodeURIComponent(String("请求接口"))}
              >
                {"请求接口"}
              </FieldLabel>
              <Input
                id={fieldId + "-field-19" + "-" + encodeURIComponent(String("请求接口"))}
                aria-label={"请求接口"}
                value={filters.path}
                onChange={(event) => setFilters({ ...filters, path: event.target.value })}
                placeholder="输入请求路径"
              />
            </Field>
            <Field>
              <FieldLabel
                htmlFor={fieldId + "-field-20" + "-" + encodeURIComponent(String("请求方法"))}
              >
                {"请求方法"}
              </FieldLabel>
              <Select
                value={filters.method}
                onValueChange={(next) =>
                  ((method) => setFilters({ ...filters, method }))(
                    next ===
                      fieldId +
                        "-field-20" +
                        "-" +
                        encodeURIComponent(String("请求方法")) +
                        "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-20" + "-" + encodeURIComponent(String("请求方法"))}
                  aria-label={"请求方法"}
                  data-empty={String(filters.method) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部方法" },
                        ...["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"].map((value) => ({
                          value,
                          label: value,
                        })),
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部方法" },
                    ...["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"].map((value) => ({
                      value,
                      label: value,
                    })),
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-20" +
                          "-" +
                          encodeURIComponent(String("请求方法")) +
                          "-empty"
                      }
                      disabled={"disabled" in option && Boolean(option.disabled)}
                    >
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field>
              <FieldLabel
                htmlFor={fieldId + "-field-21" + "-" + encodeURIComponent(String("响应结果"))}
              >
                {"响应结果"}
              </FieldLabel>
              <Select
                value={filters.result}
                onValueChange={(next) =>
                  ((result) => setFilters({ ...filters, result }))(
                    next ===
                      fieldId +
                        "-field-21" +
                        "-" +
                        encodeURIComponent(String("响应结果")) +
                        "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-21" + "-" + encodeURIComponent(String("响应结果"))}
                  aria-label={"响应结果"}
                  data-empty={String(filters.result) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部结果" },
                        { value: "success", label: "成功（2xx / 3xx）" },
                        { value: "failed", label: "失败（4xx / 5xx）" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部结果" },
                    { value: "success", label: "成功（2xx / 3xx）" },
                    { value: "failed", label: "失败（4xx / 5xx）" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-21" +
                          "-" +
                          encodeURIComponent(String("响应结果")) +
                          "-empty"
                      }
                      disabled={"disabled" in option && Boolean(option.disabled)}
                    >
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <div className="flex flex-wrap items-center gap-2 self-end">
              <Button type="submit">
                <Search />
                查询
              </Button>
              <Button
                type="button"
                variant="secondary"
                onClick={() => {
                  setFilters(empty);
                  setApplied(empty);
                  setPage(1);
                  resource.reload();
                }}
              >
                <RotateCcw />
                重置
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      {
        <>
          <Card size="sm">
            <CardHeader>
              <CardTitle role="heading" aria-level={2}>
                {"请求日志"}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
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
                        {tableColumns5.mobile ? "手机显示列" : "桌面显示列"}
                      </DropdownMenuLabel>
                      {tableColumns5.labels.map((label) => (
                        <DropdownMenuCheckboxItem
                          key={label}
                          checked={tableColumns5.isVisible(label)}
                          disabled={tableColumns5.count === 1 && tableColumns5.isVisible(label)}
                          onSelect={(event) => event.preventDefault()}
                          onCheckedChange={(checked) =>
                            tableColumns5.setVisible(label, checked === true)
                          }
                        >
                          {label}
                        </DropdownMenuCheckboxItem>
                      ))}
                      <DropdownMenuSeparator />
                      <DropdownMenuItem onSelect={tableColumns5.showAll}>
                        显示全部列
                      </DropdownMenuItem>
                      <DropdownMenuItem onSelect={tableColumns5.reset}>恢复默认列</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                <Table
                  className={
                    tableColumns5.count > 4
                      ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                      : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  }
                  role="table"
                >
                  <TableHeader>
                    <TableRow role="row">
                      {recordColumns("logs").map((label) => (
                        <TableHead
                          hidden={!tableColumns5.isVisible(label)}
                          className={
                            mobileRecordColumns("logs").includes(label) ? "max-md:w-1/2" : ""
                          }
                          key={label}
                        >
                          {label}
                        </TableHead>
                      ))}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {rows.length ? (
                      recordRows("logs", rows).map((row) => (
                        <TableRow role="row" key={row.key}>
                          {row.cells.map((cell) => (
                            <TableCell
                              hidden={!tableColumns5.isVisible(cell.label)}
                              data-label={cell.label}
                              role="cell"
                              key={cell.label}
                              className={
                                mobileRecordColumns("logs").includes(cell.label)
                                  ? "max-w-80 whitespace-normal break-words"
                                  : " max-w-80 whitespace-normal break-words"
                              }
                            >
                              <div className="max-md:hidden">
                                {cell.status ? (
                                  <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                    {cell.text}
                                  </Badge>
                                ) : (
                                  cell.text
                                )}
                              </div>
                              {cell.label === mobileRecordColumns("logs")[0] ? (
                                <RecordDialog cell={cell} row={row} />
                              ) : (
                                <div className="truncate md:hidden">
                                  {cell.status ? (
                                    <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                      {cell.text}
                                    </Badge>
                                  ) : (
                                    cell.text
                                  )}
                                </div>
                              )}
                            </TableCell>
                          ))}
                        </TableRow>
                      ))
                    ) : (
                      <TableRow role="row">
                        <TableCell role="cell" colSpan={tableColumns5.count}>
                          <Empty>
                            <EmptyDescription>暂无记录</EmptyDescription>
                          </Empty>
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              </>
              <Pagination aria-label="记录分页" className="mt-3 justify-end">
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
                  <PaginationItem className="mr-2 text-xs text-muted-foreground">
                    共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="首页"
                      {...pagination.first}
                    >
                      <ChevronsLeft />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
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
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="下一页"
                      {...pagination.next}
                    >
                      <ChevronRight />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="末页"
                      {...pagination.last}
                    >
                      <ChevronsRight />
                    </Button>
                  </PaginationItem>
                </PaginationContent>
              </Pagination>
            </CardContent>
          </Card>
          <div className="grid items-start gap-3 xl:grid-cols-2">
            <Card size="sm">
              <CardHeader>
                <CardTitle role="heading" aria-level={2}>
                  {"客户端活动"}
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
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
                          {tableColumns6.mobile ? "手机显示列" : "桌面显示列"}
                        </DropdownMenuLabel>
                        {tableColumns6.labels.map((label) => (
                          <DropdownMenuCheckboxItem
                            key={label}
                            checked={tableColumns6.isVisible(label)}
                            disabled={tableColumns6.count === 1 && tableColumns6.isVisible(label)}
                            onSelect={(event) => event.preventDefault()}
                            onCheckedChange={(checked) =>
                              tableColumns6.setVisible(label, checked === true)
                            }
                          >
                            {label}
                          </DropdownMenuCheckboxItem>
                        ))}
                        <DropdownMenuSeparator />
                        <DropdownMenuItem onSelect={tableColumns6.showAll}>
                          显示全部列
                        </DropdownMenuItem>
                        <DropdownMenuItem onSelect={tableColumns6.reset}>
                          恢复默认列
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                  <Table
                    className={
                      tableColumns6.count > 4
                        ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                        : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                    }
                    role="table"
                  >
                    <TableHeader>
                      <TableRow role="row">
                        {recordColumns("analytics").map((label) => (
                          <TableHead
                            hidden={!tableColumns6.isVisible(label)}
                            className={
                              mobileRecordColumns("analytics").includes(label) ? "max-md:w-1/2" : ""
                            }
                            key={label}
                          >
                            {label}
                          </TableHead>
                        ))}
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {(analyticsResource.data?.items ?? []).length ? (
                        recordRows("analytics", analytics.rows).map((row) => (
                          <TableRow role="row" key={row.key}>
                            {row.cells.map((cell) => (
                              <TableCell
                                hidden={!tableColumns6.isVisible(cell.label)}
                                data-label={cell.label}
                                role="cell"
                                key={cell.label}
                                className={
                                  mobileRecordColumns("analytics").includes(cell.label)
                                    ? "max-w-80 whitespace-normal break-words"
                                    : " max-w-80 whitespace-normal break-words"
                                }
                              >
                                <div className="max-md:hidden">
                                  {cell.status ? (
                                    <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                      {cell.text}
                                    </Badge>
                                  ) : (
                                    cell.text
                                  )}
                                </div>
                                {cell.label === mobileRecordColumns("analytics")[0] ? (
                                  <RecordDialog cell={cell} row={row} />
                                ) : (
                                  <div className="truncate md:hidden">
                                    {cell.status ? (
                                      <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                        {cell.text}
                                      </Badge>
                                    ) : (
                                      cell.text
                                    )}
                                  </div>
                                )}
                              </TableCell>
                            ))}
                          </TableRow>
                        ))
                      ) : (
                        <TableRow role="row">
                          <TableCell role="cell" colSpan={tableColumns6.count}>
                            <Empty>
                              <EmptyDescription>暂无记录</EmptyDescription>
                            </Empty>
                          </TableCell>
                        </TableRow>
                      )}
                    </TableBody>
                  </Table>
                </>
                <Pagination aria-label="活动分页" className="mt-3 justify-end">
                  <PaginationContent className="flex-wrap justify-end gap-1">
                    <PaginationItem>
                      <Select {...analytics.size}>
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
                    <PaginationItem className="mr-2 text-xs text-muted-foreground">
                      共 {analytics.total ?? "—"} 条 · {analytics.pages ?? "—"} 页
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="首页"
                        {...analytics.first}
                      >
                        <ChevronsLeft />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="上一页"
                        {...analytics.previous}
                      >
                        <ChevronLeft />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Input className="h-7 w-14 text-center tabular-nums" {...analytics.input} />
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="下一页"
                        {...analytics.next}
                      >
                        <ChevronRight />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="末页"
                        {...analytics.last}
                      >
                        <ChevronsRight />
                      </Button>
                    </PaginationItem>
                  </PaginationContent>
                </Pagination>
              </CardContent>
            </Card>
            <Card size="sm">
              <CardHeader>
                <CardTitle role="heading" aria-level={2}>
                  {"网站策略查询"}
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
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
                          {tableColumns7.mobile ? "手机显示列" : "桌面显示列"}
                        </DropdownMenuLabel>
                        {tableColumns7.labels.map((label) => (
                          <DropdownMenuCheckboxItem
                            key={label}
                            checked={tableColumns7.isVisible(label)}
                            disabled={tableColumns7.count === 1 && tableColumns7.isVisible(label)}
                            onSelect={(event) => event.preventDefault()}
                            onCheckedChange={(checked) =>
                              tableColumns7.setVisible(label, checked === true)
                            }
                          >
                            {label}
                          </DropdownMenuCheckboxItem>
                        ))}
                        <DropdownMenuSeparator />
                        <DropdownMenuItem onSelect={tableColumns7.showAll}>
                          显示全部列
                        </DropdownMenuItem>
                        <DropdownMenuItem onSelect={tableColumns7.reset}>
                          恢复默认列
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                  <Table
                    className={
                      tableColumns7.count > 4
                        ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                        : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                    }
                    role="table"
                  >
                    <TableHeader>
                      <TableRow role="row">
                        {recordColumns("site_status").map((label) => (
                          <TableHead
                            hidden={!tableColumns7.isVisible(label)}
                            className={
                              mobileRecordColumns("site_status").includes(label)
                                ? "max-md:w-1/2"
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
                      {(sitesResource.data?.items ?? []).length ? (
                        recordRows("site_status", sites.rows).map((row) => (
                          <TableRow role="row" key={row.key}>
                            {row.cells.map((cell) => (
                              <TableCell
                                hidden={!tableColumns7.isVisible(cell.label)}
                                data-label={cell.label}
                                role="cell"
                                key={cell.label}
                                className={
                                  mobileRecordColumns("site_status").includes(cell.label)
                                    ? "max-w-80 whitespace-normal break-words"
                                    : " max-w-80 whitespace-normal break-words"
                                }
                              >
                                <div className="max-md:hidden">
                                  {cell.status ? (
                                    <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                      {cell.text}
                                    </Badge>
                                  ) : (
                                    cell.text
                                  )}
                                </div>
                                {cell.label === mobileRecordColumns("site_status")[0] ? (
                                  <RecordDialog cell={cell} row={row} />
                                ) : (
                                  <div className="truncate md:hidden">
                                    {cell.status ? (
                                      <Badge variant={cell.failed ? "destructive" : "secondary"}>
                                        {cell.text}
                                      </Badge>
                                    ) : (
                                      cell.text
                                    )}
                                  </div>
                                )}
                              </TableCell>
                            ))}
                          </TableRow>
                        ))
                      ) : (
                        <TableRow role="row">
                          <TableCell role="cell" colSpan={tableColumns7.count}>
                            <Empty>
                              <EmptyDescription>暂无记录</EmptyDescription>
                            </Empty>
                          </TableCell>
                        </TableRow>
                      )}
                    </TableBody>
                  </Table>
                </>
                <Pagination aria-label="网站策略分页" className="mt-3 justify-end">
                  <PaginationContent className="flex-wrap justify-end gap-1">
                    <PaginationItem>
                      <Select {...sites.size}>
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
                    <PaginationItem className="mr-2 text-xs text-muted-foreground">
                      共 {sites.total ?? "—"} 条 · {sites.pages ?? "—"} 页
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="首页"
                        {...sites.first}
                      >
                        <ChevronsLeft />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="上一页"
                        {...sites.previous}
                      >
                        <ChevronLeft />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Input className="h-7 w-14 text-center tabular-nums" {...sites.input} />
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="下一页"
                        {...sites.next}
                      >
                        <ChevronRight />
                      </Button>
                    </PaginationItem>
                    <PaginationItem>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="末页"
                        {...sites.last}
                      >
                        <ChevronsRight />
                      </Button>
                    </PaginationItem>
                  </PaginationContent>
                </Pagination>
              </CardContent>
            </Card>
          </div>
        </>
      }
    </>
  );
}
