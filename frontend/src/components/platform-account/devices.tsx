"use client";
import { DeviceDialog } from "@/components/platform-account/device-dialog";
import { RecordDialog } from "@/components/platform-account/record-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
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
import { scopes } from "@/lib/domain";
import { date } from "@/lib/format";
import { useDevices } from "@/lib/platform-account";
import { mobileRecordColumns, recordColumns, recordRows } from "@/lib/records";
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Columns3 } from "lucide-react";

export function Devices({ id }: { id: string }) {
  const {
    tableColumns2,
    tableColumns3,
    actions,
    resource,
    serverResource,
    devices,
    servers,
    handleClick,
  } = useDevices({ id });
  return (
    <>
      {
        <>
          <Card size="sm">
            <CardHeader>
              <CardTitle role="heading" aria-level={2}>
                {"登录设备与授权范围"}
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
                        {tableColumns2.mobile ? "手机显示列" : "桌面显示列"}
                      </DropdownMenuLabel>
                      {tableColumns2.labels.map((label) => (
                        <DropdownMenuCheckboxItem
                          key={label}
                          checked={tableColumns2.isVisible(label)}
                          disabled={tableColumns2.count === 1 && tableColumns2.isVisible(label)}
                          onSelect={(event) => event.preventDefault()}
                          onCheckedChange={(checked) =>
                            tableColumns2.setVisible(label, checked === true)
                          }
                        >
                          {label}
                        </DropdownMenuCheckboxItem>
                      ))}
                      <DropdownMenuSeparator />
                      <DropdownMenuItem onSelect={tableColumns2.showAll}>
                        显示全部列
                      </DropdownMenuItem>
                      <DropdownMenuItem onSelect={tableColumns2.reset}>恢复默认列</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                <Table
                  className={
                    tableColumns2.count > 4
                      ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                      : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  }
                  role="table"
                >
                  <TableHeader>
                    <TableRow role="row">
                      {["客户端 / 安装标识", "授权范围", "首次登录", "最近续期 / 使用", "操作"].map(
                        (label) => (
                          <TableHead
                            hidden={!tableColumns2.isVisible(label)}
                            className={
                              ["客户端 / 安装标识", "最近续期 / 使用", "操作"].includes(label)
                                ? label === "操作"
                                  ? "max-md:w-28"
                                  : label === "客户端 / 安装标识"
                                    ? ""
                                    : "max-md:w-16"
                                : ""
                            }
                            key={label}
                            scope="col"
                          >
                            {label}
                          </TableHead>
                        ),
                      )}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {(resource.data?.items ?? []).length ? (
                      <>
                        {devices.rows.map((device) => (
                          <TableRow role="row" key={device.id}>
                            <TableCell
                              hidden={!tableColumns2.isVisible("客户端 / 安装标识")}
                              data-label="客户端 / 安装标识"
                              role="cell"
                              className="max-w-80 whitespace-normal break-words max-md:overflow-hidden"
                            >
                              <div className="max-md:hidden">
                                {device.user_agent || "未知客户端"}
                                <CardDescription className="break-all font-mono text-xs">
                                  {device.installation_id ?? "未提供安装标识"}
                                </CardDescription>
                              </div>
                              <DeviceDialog device={device} />
                            </TableCell>
                            <TableCell
                              hidden={!tableColumns2.isVisible("授权范围")}
                              className=" "
                              data-label="授权范围"
                              role="cell"
                            >
                              {device.scopes
                                .split(/\s+/)
                                .map((scope) => scopes[scope] ?? scope)
                                .join("、")}
                            </TableCell>
                            <TableCell
                              hidden={!tableColumns2.isVisible("首次登录")}
                              className=" "
                              data-label="首次登录"
                              role="cell"
                            >
                              {date(device.authenticated_at_ms ?? device.created_at)}
                            </TableCell>
                            <TableCell
                              hidden={!tableColumns2.isVisible("最近续期 / 使用")}
                              className=" max-md:overflow-hidden"
                              data-label="最近续期 / 使用"
                              role="cell"
                            >
                              {date(device.last_login_at)}
                              <CardDescription>{date(device.last_used_at)}</CardDescription>
                            </TableCell>
                            <TableCell
                              hidden={!tableColumns2.isVisible("操作")}
                              className=" max-md:[&_button]:h-7 max-md:[&_button]:px-1.5 max-md:[&_button]:text-xs max-md:[&_button]:gap-1 max-md:[&_a]:h-7 max-md:[&_a]:px-1.5 max-md:[&_a]:text-xs max-md:[&_a]:gap-1 max-md:[&>div]:gap-1"
                              data-label="操作"
                              role="cell"
                            >
                              <Button
                                type="button"
                                variant="destructive"
                                disabled={
                                  false || actions.isBusy("components\\consumers.tsx:action:17")
                                }
                                onClick={() => handleClick(device)}
                              >
                                撤销授权
                              </Button>
                            </TableCell>
                          </TableRow>
                        ))}
                      </>
                    ) : (
                      <TableRow role="row">
                        <TableCell role="cell" colSpan={tableColumns2.count}>
                          <Empty>
                            <EmptyDescription>{"暂无记录"}</EmptyDescription>
                          </Empty>
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              </>
              <Pagination aria-label="登录设备分页" className="mt-3 justify-end">
                <PaginationContent className="flex-wrap justify-end gap-1">
                  <PaginationItem>
                    <Select {...devices.size}>
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
                    共 {devices.total ?? "—"} 条 · {devices.pages ?? "—"} 页
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="首页"
                      {...devices.first}
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
                      {...devices.previous}
                    >
                      <ChevronLeft />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Input className="h-7 w-14 text-center tabular-nums" {...devices.input} />
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="下一页"
                      {...devices.next}
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
                      {...devices.last}
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
                {"远程主机注册"}
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
                        {tableColumns3.mobile ? "手机显示列" : "桌面显示列"}
                      </DropdownMenuLabel>
                      {tableColumns3.labels.map((label) => (
                        <DropdownMenuCheckboxItem
                          key={label}
                          checked={tableColumns3.isVisible(label)}
                          disabled={tableColumns3.count === 1 && tableColumns3.isVisible(label)}
                          onSelect={(event) => event.preventDefault()}
                          onCheckedChange={(checked) =>
                            tableColumns3.setVisible(label, checked === true)
                          }
                        >
                          {label}
                        </DropdownMenuCheckboxItem>
                      ))}
                      <DropdownMenuSeparator />
                      <DropdownMenuItem onSelect={tableColumns3.showAll}>
                        显示全部列
                      </DropdownMenuItem>
                      <DropdownMenuItem onSelect={tableColumns3.reset}>恢复默认列</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                <Table
                  className={
                    tableColumns3.count > 4
                      ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                      : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
                  }
                  role="table"
                >
                  <TableHeader>
                    <TableRow role="row">
                      {recordColumns("remote_servers").map((label) => (
                        <TableHead
                          hidden={!tableColumns3.isVisible(label)}
                          className={
                            mobileRecordColumns("remote_servers").includes(label)
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
                    {(serverResource.data?.items ?? []).length ? (
                      recordRows("remote_servers", servers.rows).map((row) => (
                        <TableRow role="row" key={row.key}>
                          {row.cells.map((cell) => (
                            <TableCell
                              hidden={!tableColumns3.isVisible(cell.label)}
                              data-label={cell.label}
                              role="cell"
                              key={cell.label}
                              className={
                                mobileRecordColumns("remote_servers").includes(cell.label)
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
                              {cell.label === mobileRecordColumns("remote_servers")[0] ? (
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
                        <TableCell role="cell" colSpan={tableColumns3.count}>
                          <Empty>
                            <EmptyDescription>暂无记录</EmptyDescription>
                          </Empty>
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              </>
              <Pagination aria-label="远程主机分页" className="mt-3 justify-end">
                <PaginationContent className="flex-wrap justify-end gap-1">
                  <PaginationItem>
                    <Select {...servers.size}>
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
                    共 {servers.total ?? "—"} 条 · {servers.pages ?? "—"} 页
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="首页"
                      {...servers.first}
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
                      {...servers.previous}
                    >
                      <ChevronLeft />
                    </Button>
                  </PaginationItem>
                  <PaginationItem>
                    <Input className="h-7 w-14 text-center tabular-nums" {...servers.input} />
                  </PaginationItem>
                  <PaginationItem>
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-sm"
                      aria-label="下一页"
                      {...servers.next}
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
                      {...servers.last}
                    >
                      <ChevronsRight />
                    </Button>
                  </PaginationItem>
                </PaginationContent>
              </Pagination>
            </CardContent>
          </Card>
        </>
      }
    </>
  );
}
