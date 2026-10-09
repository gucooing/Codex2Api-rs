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
import { useRecords } from "@/lib/platform-account";
import { mobileRecordColumns, recordColumns, recordRows } from "@/lib/records";
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Columns3 } from "lucide-react";

export function Records({ id, kind, title }: { id: string; kind: string; title: string }) {
  const { tableColumns4, resource, pagination } = useRecords({ id, kind });
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {
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
                    {tableColumns4.mobile ? "手机显示列" : "桌面显示列"}
                  </DropdownMenuLabel>
                  {tableColumns4.labels.map((label) => (
                    <DropdownMenuCheckboxItem
                      key={label}
                      checked={tableColumns4.isVisible(label)}
                      disabled={tableColumns4.count === 1 && tableColumns4.isVisible(label)}
                      onSelect={(event) => event.preventDefault()}
                      onCheckedChange={(checked) =>
                        tableColumns4.setVisible(label, checked === true)
                      }
                    >
                      {label}
                    </DropdownMenuCheckboxItem>
                  ))}
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onSelect={tableColumns4.showAll}>显示全部列</DropdownMenuItem>
                  <DropdownMenuItem onSelect={tableColumns4.reset}>恢复默认列</DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
            <Table
              className={
                tableColumns4.count > 4
                  ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
              }
              role="table"
            >
              <TableHeader>
                <TableRow role="row">
                  {recordColumns(kind).map((label) => (
                    <TableHead
                      hidden={!tableColumns4.isVisible(label)}
                      className={mobileRecordColumns(kind).includes(label) ? "max-md:w-1/2" : ""}
                      key={label}
                    >
                      {label}
                    </TableHead>
                  ))}
                </TableRow>
              </TableHeader>
              <TableBody>
                {(resource.data?.items ?? []).length ? (
                  recordRows(kind, pagination.rows).map((row) => (
                    <TableRow role="row" key={row.key}>
                      {row.cells.map((cell) => (
                        <TableCell
                          hidden={!tableColumns4.isVisible(cell.label)}
                          data-label={cell.label}
                          role="cell"
                          key={cell.label}
                          className={
                            mobileRecordColumns(kind).includes(cell.label)
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
                          {cell.label === mobileRecordColumns(kind)[0] ? (
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
                    <TableCell role="cell" colSpan={tableColumns4.count}>
                      <Empty>
                        <EmptyDescription>暂无记录</EmptyDescription>
                      </Empty>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </>
        }
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
  );
}
