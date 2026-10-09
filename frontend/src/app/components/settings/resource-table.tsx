"use client";
import { ResourceRecordDialog } from "@/app/components/settings/resource-record-dialog";
import { useResourceTable } from "@/app/data/settings";
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
import { date } from "@/lib/format";
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Columns3 } from "lucide-react";

export function ResourceTable({
  path,
  title,
  columns,
  fields,
}: {
  path: string;
  title: string;
  columns: string[];
  fields: [string, "date"?][];
}) {
  const { tableColumns0, resource, pagination, items } = useResourceTable({ path, columns });
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {!resource.ready && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={resource.reload}
            disabled={resource.refreshing}
          >
            重新加载
          </Button>
        )}
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
            className={
              tableColumns0.count > 4
                ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
            }
            role="table"
          >
            <TableHeader>
              <TableRow role="row">
                {columns.map((label) => (
                  <TableHead
                    hidden={!tableColumns0.isVisible(label)}
                    className={columns.indexOf(label) < 2 ? "max-md:w-1/2" : ""}
                    key={label}
                    scope="col"
                  >
                    {label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {items.length ? (
                <>
                  {items.map((item, index) => (
                    <TableRow role="row" key={String(item.id ?? index)}>
                      {fields.map(([field, format], fieldIndex) => (
                        <TableCell
                          hidden={!tableColumns0.isVisible(columns[fieldIndex])}
                          role="cell"
                          data-label={columns[fieldIndex]}
                          key={field}
                          className={
                            fieldIndex < 2
                              ? "max-w-80 whitespace-normal break-words"
                              : " max-w-80 whitespace-normal break-words"
                          }
                        >
                          <div className="max-md:hidden">
                            {format === "date"
                              ? date(
                                  typeof item[field] === "number" || typeof item[field] === "string"
                                    ? (item[field] as string | number)
                                    : null,
                                )
                              : item[field] == null
                                ? "—"
                                : String(item[field])}
                          </div>
                          {fieldIndex === 0 ? (
                            <ResourceRecordDialog
                              format={format}
                              item={item}
                              field={field}
                              title={title}
                              fields={fields}
                              columns={columns}
                            />
                          ) : (
                            <div className="truncate md:hidden">
                              {format === "date"
                                ? date(
                                    typeof item[field] === "number" ||
                                      typeof item[field] === "string"
                                      ? (item[field] as string | number)
                                      : null,
                                  )
                                : item[field] == null
                                  ? "—"
                                  : String(item[field])}
                            </div>
                          )}
                        </TableCell>
                      ))}
                    </TableRow>
                  ))}
                </>
              ) : (
                <TableRow role="row">
                  <TableCell role="cell" colSpan={tableColumns0.count}>
                    <Empty>
                      <EmptyDescription>
                        {resource.loading
                          ? "正在加载…"
                          : resource.error
                            ? "尚未取得记录"
                            : "暂无记录"}
                      </EmptyDescription>
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
  );
}
