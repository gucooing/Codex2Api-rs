"use client";
import { BrowserPolicyDialog } from "@/components/platform-account/client-state/browser-policy-dialog";
import { Button } from "@/components/ui/button";
import { CardTitle } from "@/components/ui/card";
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
  clientLabels,
  clientScalar,
  useBrowserClientState,
} from "@/lib/platform-account/client-state";
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Columns3 } from "lucide-react";

export function BrowserClientState({ value, path }: { value: Json; path: string }) {
  const { tableColumns0, preferences, pages } = useBrowserClientState({ value, path });
  return (
    <div className="space-y-4">
      <CardTitle role="heading" aria-level={3}>
        审批偏好
      </CardTitle>
      <FieldGroup className="grid gap-4 sm:grid-cols-2 gap-3">
        {Object.entries(preferences)
          .filter(([key]) => key in clientLabels)
          .map(([key, value]) => ({ label: clientLabels[key], value: clientScalar(value) }))
          .map(({ label, value }) => (
            <Field key={label}>
              <FieldTitle>{label}</FieldTitle>
              <FieldDescription>{value ?? "—"}</FieldDescription>
            </Field>
          ))}
      </FieldGroup>
      {(
        [
          ["origin", "站点访问"],
          ["download", "下载"],
          ["upload", "上传"],
          ["full_cdp", "完整浏览器控制"],
        ] as const
      ).map(([key, title]) => {
        const pagination = pages[key];
        const rows = pagination.rows;
        return (
          <section key={key}>
            <CardTitle role="heading" aria-level={3}>
              {title}规则
            </CardTitle>
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
                        onCheckedChange={(checked) =>
                          tableColumns0.setVisible(label, checked === true)
                        }
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
                    {["站点匹配规则", "审批策略"].map((label) => (
                      <TableHead
                        hidden={!tableColumns0.isVisible(label)}
                        className={
                          ["站点匹配规则", "审批策略"].includes(label)
                            ? label === "站点匹配规则"
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
                  {rows.length ? (
                    <>
                      {rows.map(([site, policy]) => (
                        <TableRow role="row" key={site}>
                          <TableCell
                            hidden={!tableColumns0.isVisible("站点匹配规则")}
                            data-label="站点匹配规则"
                            role="cell"
                            className="max-w-80 whitespace-normal break-words max-md:overflow-hidden"
                          >
                            <div className="max-md:hidden">{site}</div>
                            <BrowserPolicyDialog site={site} policy={policy} />
                          </TableCell>
                          <TableCell
                            hidden={!tableColumns0.isVisible("审批策略")}
                            className=" max-md:overflow-hidden"
                            data-label="审批策略"
                            role="cell"
                          >
                            {clientScalar(policy)}
                          </TableCell>
                        </TableRow>
                      ))}
                    </>
                  ) : (
                    <TableRow role="row">
                      <TableCell role="cell" colSpan={tableColumns0.count}>
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
          </section>
        );
      })}
    </div>
  );
}
