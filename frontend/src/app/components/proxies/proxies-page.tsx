"use client";
import { ProxyEditor } from "@/app/components/proxies/proxy-editor";
import { ProxyRecordDialog } from "@/app/components/proxies/proxy-record-dialog";
import { useProxiesPage } from "@/app/data/proxies";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription } from "@/components/ui/card";
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
import { date } from "@/lib/format";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  MoreHorizontal,
  Pencil,
  Plus,
  RotateCcw,
  Search,
} from "lucide-react";

export default function ProxiesPage() {
  const {
    tableColumns0,
    fieldId,
    actions,
    editing,
    setEditing,
    empty,
    filters,
    setFilters,
    setApplied,
    resource,
    pagination,
    handleClick,
    handleSelect2,
    handleSelect3,
    handleSelect4,
    handleSelect5,
  } = useProxiesPage();
  return (
    <>
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters });
              resource.reload(1);
            }}
          >
            <Field className="w-40">
              <FieldLabel
                htmlFor={fieldId + "-field-1" + "-" + encodeURIComponent(String("搜索代理"))}
              >
                {"搜索代理"}
              </FieldLabel>
              <Input
                id={fieldId + "-field-1" + "-" + encodeURIComponent(String("搜索代理"))}
                aria-label={"搜索代理"}
                value={filters.search}
                onChange={(event) => setFilters({ ...filters, search: event.target.value })}
                placeholder="名称或主机地址"
              />
            </Field>
            <Field className="w-40">
              <FieldLabel
                htmlFor={fieldId + "-field-2" + "-" + encodeURIComponent(String("代理协议"))}
              >
                {"代理协议"}
              </FieldLabel>
              <Select
                value={filters.protocol}
                onValueChange={(next) =>
                  ((protocol) => setFilters({ ...filters, protocol }))(
                    next ===
                      fieldId + "-field-2" + "-" + encodeURIComponent(String("代理协议")) + "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-2" + "-" + encodeURIComponent(String("代理协议"))}
                  aria-label={"代理协议"}
                  data-empty={String(filters.protocol) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部协议" },
                        ...["http", "https", "socks5", "socks5h"].map((value) => ({
                          value,
                          label: value.toUpperCase(),
                        })),
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部协议" },
                    ...["http", "https", "socks5", "socks5h"].map((value) => ({
                      value,
                      label: value.toUpperCase(),
                    })),
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-2" +
                          "-" +
                          encodeURIComponent(String("代理协议")) +
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
            <Field className="w-40">
              <FieldLabel
                htmlFor={fieldId + "-field-3" + "-" + encodeURIComponent(String("连接检查结果"))}
              >
                {"连接检查结果"}
              </FieldLabel>
              <Select
                value={filters.result}
                onValueChange={(next) =>
                  ((result) => setFilters({ ...filters, result }))(
                    next ===
                      fieldId +
                        "-field-3" +
                        "-" +
                        encodeURIComponent(String("连接检查结果")) +
                        "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-3" + "-" + encodeURIComponent(String("连接检查结果"))}
                  aria-label={"连接检查结果"}
                  data-empty={String(filters.result) === "" ? "true" : undefined}
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "", label: "全部结果" },
                        { value: "success", label: "最近检查成功" },
                        { value: "failed", label: "最近检查失败" },
                        { value: "unchecked", label: "尚未检查" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "", label: "全部结果" },
                    { value: "success", label: "最近检查成功" },
                    { value: "failed", label: "最近检查失败" },
                    { value: "unchecked", label: "尚未检查" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-3" +
                          "-" +
                          encodeURIComponent(String("连接检查结果")) +
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
                  resource.reload(1);
                }}
              >
                <RotateCcw />
                重置
              </Button>
            </div>
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
          </form>
          <div className="flex flex-wrap items-center gap-2 self-end xl:ml-auto">
            {
              <Button type="button" onClick={() => setEditing("new")}>
                <Plus />
                添加代理
              </Button>
            }
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="space-y-4">
          {
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
                  {["代理", "绑定账户", "出口 / 时区", "连接检查", "质量检查", "操作"].map(
                    (label) => (
                      <TableHead
                        hidden={!tableColumns0.isVisible(label)}
                        className={
                          ["代理", "连接检查", "操作"].includes(label)
                            ? label === "操作"
                              ? "max-md:w-28"
                              : label === "代理"
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
                {pagination.rows.length ? (
                  <>
                    {pagination.rows.map((proxy) => (
                      <TableRow role="row" key={proxy.id}>
                        <TableCell
                          hidden={!tableColumns0.isVisible("代理")}
                          className=" max-md:overflow-hidden"
                          data-label="代理"
                          role="cell"
                        >
                          <div className="max-md:hidden">
                            <strong>{proxy.name}</strong>
                            <CardDescription>{proxy.display_url}</CardDescription>
                          </div>
                          <ProxyRecordDialog proxy={proxy} />
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("绑定账户")}
                          className=" "
                          data-label="绑定账户"
                          data-compact="true"
                          role="cell"
                        >
                          {proxy.account_count}
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("出口 / 时区")}
                          className=" "
                          data-label="出口 / 时区"
                          role="cell"
                        >
                          {[proxy.exit_ip, proxy.country, proxy.region, proxy.city]
                            .filter(Boolean)
                            .join(" · ") || "未检查"}
                          <CardDescription>{proxy.timezone ?? "时区未获取"}</CardDescription>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("连接检查")}
                          className=" max-md:overflow-hidden"
                          data-label="连接检查"
                          role="cell"
                        >
                          <div className="max-md:hidden">
                            {proxy.connection_ok === null ? (
                              <span className="text-sm text-muted-foreground">未检查</span>
                            ) : (
                              <Badge variant={proxy.connection_ok ? "secondary" : "outline"}>
                                {proxy.connection_ok ? "检查成功" : "连接失败"}
                              </Badge>
                            )}
                            <CardDescription>
                              {proxy.connection_latency_ms === null
                                ? ""
                                : `${proxy.connection_latency_ms} ms`}
                            </CardDescription>
                            <CardDescription className="max-w-80 whitespace-normal break-words">
                              {proxy.connection_error}
                            </CardDescription>
                            <CardDescription>{date(proxy.connection_checked_at)}</CardDescription>
                          </div>
                          <div className="md:hidden">
                            <Badge variant={proxy.connection_ok ? "secondary" : "outline"}>
                              {proxy.connection_ok === null
                                ? "未检查"
                                : proxy.connection_ok
                                  ? "成功"
                                  : "失败"}
                            </Badge>
                          </div>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("质量检查")}
                          className=" "
                          data-label="质量检查"
                          role="cell"
                        >
                          {proxy.quality_ok === null ? (
                            <span className="text-sm text-muted-foreground">未检查</span>
                          ) : (
                            <Badge variant={proxy.quality_ok ? "secondary" : "outline"}>
                              {proxy.quality_ok ? "检查通过" : "检查失败"}
                            </Badge>
                          )}
                          <CardDescription>
                            {proxy.quality_latency_ms === null
                              ? ""
                              : `${proxy.quality_latency_ms} ms`}
                          </CardDescription>
                          <CardDescription className="max-w-80 whitespace-normal break-words">
                            {proxy.quality_error}
                          </CardDescription>
                          <CardDescription>{date(proxy.quality_checked_at)}</CardDescription>
                        </TableCell>
                        <TableCell
                          hidden={!tableColumns0.isVisible("操作")}
                          className=" max-md:[&_button]:h-7 max-md:[&_button]:px-1.5 max-md:[&_button]:text-xs max-md:[&_button]:gap-1 max-md:[&_a]:h-7 max-md:[&_a]:px-1.5 max-md:[&_a]:text-xs max-md:[&_a]:gap-1 max-md:[&>div]:gap-1"
                          data-label="操作"
                          role="cell"
                        >
                          <div className="flex flex-wrap items-center gap-2">
                            <Button variant="outline" size="sm" onClick={() => setEditing(proxy)}>
                              <Pencil />
                              编辑
                            </Button>
                            <Button
                              className="max-md:hidden"
                              type="button"
                              variant="outline"
                              disabled={false || actions.isBusy("app\\proxies\\page.tsx:action:4")}
                              onClick={() => handleClick(proxy)}
                            >
                              检查连接
                            </Button>
                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                                <Button
                                  type="button"
                                  variant="ghost"
                                  size="icon-sm"
                                  aria-label="更多操作"
                                >
                                  <MoreHorizontal />
                                </Button>
                              </DropdownMenuTrigger>
                              <DropdownMenuContent align="end">
                                <DropdownMenuItem
                                  className="md:hidden"
                                  disabled={actions.isBusy("app\\proxies\\page.tsx:action:4")}
                                  onSelect={() => handleSelect2(proxy)}
                                >
                                  检查连接
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  variant="default"
                                  disabled={
                                    false || actions.isBusy("app\\proxies\\page.tsx:action:5")
                                  }
                                  onSelect={() => handleSelect3(proxy)}
                                >
                                  检查质量
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  variant="default"
                                  disabled={
                                    false || actions.isBusy("app\\proxies\\page.tsx:action:6")
                                  }
                                  onSelect={() => handleSelect4(proxy)}
                                >
                                  获取时区
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  variant="destructive"
                                  disabled={
                                    false || actions.isBusy("app\\proxies\\page.tsx:action:7")
                                  }
                                  onSelect={() => handleSelect5(proxy)}
                                >
                                  删除代理
                                </DropdownMenuItem>
                              </DropdownMenuContent>
                            </DropdownMenu>
                          </div>
                        </TableCell>
                      </TableRow>
                    ))}
                  </>
                ) : (
                  <TableRow role="row">
                    <TableCell role="cell" colSpan={tableColumns0.count}>
                      <Empty>
                        <EmptyDescription>{"暂无符合条件的代理"}</EmptyDescription>
                      </Empty>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
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
      {editing && (
        <ProxyEditor
          proxy={editing === "new" ? undefined : editing}
          onClose={() => setEditing(undefined)}
          onSaved={() => {
            setEditing(undefined);
            resource.reload();
          }}
        />
      )}
    </>
  );
}
