"use client";
import { ClientStateRecordDialog } from "@/components/platform-account/client-state/client-state-record-dialog";
import { Button } from "@/components/ui/button";
import { CardDescription, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
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
import { type Json } from "@/lib/api";
import {
  clientGroups,
  clientScalar,
  useNamedClientState,
} from "@/lib/platform-account/client-state";
import { obj } from "@/lib/platform-account/config";
import { scalar } from "@/lib/records";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  Inbox,
} from "lucide-react";

export function NamedClientState({
  value,
  path,
  section = "$",
}: {
  value: Json;
  path: string;
  section?: string;
}) {
  const { tableColumns1, pagination, entries, fields } = useNamedClientState({
    value,
    path,
    section,
  });
  if (Array.isArray(value))
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
                  {tableColumns1.mobile ? "手机显示列" : "桌面显示列"}
                </DropdownMenuLabel>
                {tableColumns1.labels.map((label) => (
                  <DropdownMenuCheckboxItem
                    key={label}
                    checked={tableColumns1.isVisible(label)}
                    disabled={tableColumns1.count === 1 && tableColumns1.isVisible(label)}
                    onSelect={(event) => event.preventDefault()}
                    onCheckedChange={(checked) => tableColumns1.setVisible(label, checked === true)}
                  >
                    {label}
                  </DropdownMenuCheckboxItem>
                ))}
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={tableColumns1.showAll}>显示全部列</DropdownMenuItem>
                <DropdownMenuItem onSelect={tableColumns1.reset}>恢复默认列</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          <Table
            className={
              tableColumns1.count > 4
                ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
            }
            role="table"
          >
            <TableHeader>
              <TableRow role="row">
                {["名称 / 内容", "标识", "状态", "详细记录"].map((label) => (
                  <TableHead
                    hidden={!tableColumns1.isVisible(label)}
                    className={
                      ["名称 / 内容", "状态"].includes(label)
                        ? label === "名称 / 内容"
                          ? ""
                          : "max-md:w-20"
                        : ""
                    }
                    key={label}
                    scope="col"
                  >
                    {label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {pagination.rows.length ? (
                <>
                  {pagination.rows.map((raw, index) => {
                    const row = obj(raw);
                    return (
                      <TableRow role="row" key={index}>
                        <TableCell
                          hidden={!tableColumns1.isVisible("名称 / 内容")}
                          className=" max-md:overflow-hidden"
                          data-label="名称 / 内容"
                          role="cell"
                        >
                          <div className="max-md:hidden">
                            {typeof raw === "string"
                              ? raw
                              : scalar(
                                  row.title ??
                                    row.name ??
                                    row.email ??
                                    row.category ??
                                    obj(row.gizmo).name,
                                )}
                          </div>
                          <ClientStateRecordDialog
                            raw={raw}
                            row={row}
                            path={path}
                            section={section}
                            pagination={pagination}
                            index={index}
                          />
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns1.isVisible("标识")}
                          data-label="标识"
                          role="cell"
                          className="break-all font-mono text-xs "
                        >
                          {scalar(row.id ?? row.item_id ?? obj(row.gizmo).id)}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns1.isVisible("状态")}
                          className=" max-md:overflow-hidden"
                          data-label="状态"
                          data-compact="true"
                          role="cell"
                        >
                          {clientScalar(row.status ?? row.enabled)}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns1.isVisible("详细记录")}
                          className=" "
                          data-label="详细记录"
                          role="cell"
                        >
                          {raw && typeof raw === "object" && !Array.isArray(raw) && (
                            <Collapsible>
                              <CollapsibleTrigger asChild>
                                <Button type="button" variant="ghost" size="sm">
                                  查看详情
                                  <ChevronDown />
                                </Button>
                              </CollapsibleTrigger>
                              <CollapsibleContent>
                                <NamedClientState
                                  value={raw}
                                  path={path}
                                  section={`${section}[${(pagination.page - 1) * Number(pagination.size.value) + index}]`}
                                />
                              </CollapsibleContent>
                            </Collapsible>
                          )}
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </>
              ) : (
                <TableRow role="row">
                  <TableCell role="cell" colSpan={tableColumns1.count}>
                    <Empty>
                      <EmptyDescription>{"暂无记录"}</EmptyDescription>
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
      </>
    );
  if (!value || typeof value !== "object")
    return <CardDescription>{clientScalar(value)}</CardDescription>;
  return (
    <div className="space-y-4">
      {fields.length > 0 && (
        <FieldGroup className="grid gap-4 sm:grid-cols-2 gap-3">
          {fields.map(({ label, value }) => (
            <Field key={label}>
              <FieldTitle>{label}</FieldTitle>
              <FieldDescription>{value ?? "—"}</FieldDescription>
            </Field>
          ))}
        </FieldGroup>
      )}
      {entries
        .filter(([, item]) => item !== null && typeof item === "object")
        .map(([key, item]) => (
          <section key={key}>
            {clientGroups[key] && (
              <CardTitle role="heading" aria-level={3}>
                {clientGroups[key]}
              </CardTitle>
            )}
            <NamedClientState
              value={item}
              path={path}
              section={`${section}.${JSON.stringify(key)}`}
            />
          </section>
        ))}
      {!fields.length && !entries.some(([, item]) => item !== null && typeof item === "object") && (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Inbox />
            </EmptyMedia>
            <EmptyDescription>暂无已同步的具名业务记录</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </div>
  );
}
