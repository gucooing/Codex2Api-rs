"use client";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useTablePagination } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight } from "lucide-react";
import { useDialogFocus } from "@/lib/actions";
import { ScrollArea } from "@/components/ui/scroll-area";

import { Card, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel, FieldSet, FieldGroup } from "@/components/ui/field";
import { useId } from "react";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableBody,
  TableCell,
} from "@/components/ui/table";
import { Empty, EmptyDescription } from "@/components/ui/empty";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from "@/components/ui/dropdown-menu";
import { MoreHorizontal, X, Info } from "lucide-react";
import { useActions, useErrorToast } from "@/lib/actions";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogClose,
} from "@/components/ui/dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { date } from "@/lib/format";
import { useState } from "react";
import { Plus, Pencil, Search, RotateCcw } from "lucide-react";
import { request, type List, type Proxy, type ProxyWrite } from "@/lib/api";
import { useResource } from "@/lib/hooks";

export default function ProxiesPage() {
  const fieldId = useId();
  const actions = useActions();
  const resource = useResource<List<Proxy>>("/proxies");
  const [editing, setEditing] = useState<Proxy | "new">();
  const empty = { search: "", protocol: "", result: "" };
  const [filters, setFilters] = useState(empty);
  const [applied, setApplied] = useState(empty);
  const items =
    resource.data?.items.filter(
      (proxy) =>
        `${proxy.name} ${proxy.host}`.toLowerCase().includes(applied.search.trim().toLowerCase()) &&
        (!applied.protocol || proxy.protocol === applied.protocol) &&
        (!applied.result ||
          (applied.result === "unchecked"
            ? proxy.connection_ok === null
            : proxy.connection_ok === (applied.result === "success"))),
    ) ?? [];
  const pagination = useTablePagination(items, applied, resource.data !== undefined);
  const check = async (proxy: Proxy, action: string) => {
    await request(`/proxies/${proxy.id}/check/${action}`, { method: "POST", body: {} });
    resource.reload();
  };
  useErrorToast(resource.error);
  return (
    <>
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied({ ...filters });
              resource.reload();
            }}
          >
            <Field className="w-44">
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
            <Field className="w-44">
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
                  data-required={false ? "true" : undefined}
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
            <Field className="w-44">
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
                  data-required={false ? "true" : undefined}
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
                  resource.reload();
                }}
              >
                <RotateCcw />
                重置
              </Button>
            </div>
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
            <Table>
              <TableHeader>
                <TableRow>
                  {["代理", "绑定账户", "出口 / 时区", "连接检查", "质量检查", "操作"].map(
                    (label) => (
                      <TableHead key={label} scope="col">
                        {label}
                      </TableHead>
                    ),
                  )}
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.length ? (
                  <>
                    {pagination.rows.map((proxy) => (
                      <TableRow key={proxy.id}>
                        <TableCell>
                          <strong>{proxy.name}</strong>
                          <CardDescription>{proxy.display_url}</CardDescription>
                        </TableCell>
                        <TableCell>{proxy.account_count}</TableCell>
                        <TableCell>
                          {[proxy.exit_ip, proxy.country, proxy.region, proxy.city]
                            .filter(Boolean)
                            .join(" · ") || "未检查"}
                          <CardDescription>{proxy.timezone ?? "时区未获取"}</CardDescription>
                        </TableCell>
                        <TableCell>
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
                        </TableCell>
                        <TableCell>
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
                        <TableCell>
                          <div className="flex flex-wrap items-center gap-2">
                            <Button variant="outline" size="sm" onClick={() => setEditing(proxy)}>
                              <Pencil />
                              编辑
                            </Button>
                            <Button
                              type="button"
                              variant={false ? "destructive" : "outline"}
                              disabled={false || actions.isBusy("app\\proxies\\page.tsx:action:4")}
                              onClick={() =>
                                void actions.run(
                                  "app\\proxies\\page.tsx:action:4",
                                  () => check(proxy, "test"),
                                  { confirm: undefined, danger: false, success: undefined },
                                )
                              }
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
                                  variant={false ? "destructive" : "default"}
                                  disabled={
                                    false || actions.isBusy("app\\proxies\\page.tsx:action:5")
                                  }
                                  onSelect={() =>
                                    void actions.run(
                                      "app\\proxies\\page.tsx:action:5",
                                      () => check(proxy, "quality"),
                                      { confirm: undefined, danger: false, success: undefined },
                                    )
                                  }
                                >
                                  检查质量
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  variant={false ? "destructive" : "default"}
                                  disabled={
                                    false || actions.isBusy("app\\proxies\\page.tsx:action:6")
                                  }
                                  onSelect={() =>
                                    void actions.run(
                                      "app\\proxies\\page.tsx:action:6",
                                      () => check(proxy, "timezone"),
                                      { confirm: undefined, danger: false, success: undefined },
                                    )
                                  }
                                >
                                  获取时区
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  variant={true ? "destructive" : "default"}
                                  disabled={
                                    false || actions.isBusy("app\\proxies\\page.tsx:action:7")
                                  }
                                  onSelect={() =>
                                    void actions.run(
                                      "app\\proxies\\page.tsx:action:7",
                                      async () => {
                                        await request(`/proxies/${proxy.id}`, {
                                          method: "DELETE",
                                          body: { confirm_unbind: proxy.account_count > 0 },
                                        });
                                        resource.reload();
                                      },
                                      {
                                        confirm: proxy.account_count
                                          ? `此代理关联 ${proxy.account_count} 个供应账户，删除并解除绑定？`
                                          : "删除此代理？",
                                        danger: true,
                                        success: "代理已删除",
                                      },
                                    )
                                  }
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
                  <TableRow>
                    <TableCell
                      colSpan={
                        ["代理", "绑定账户", "出口 / 时区", "连接检查", "质量检查", "操作"].length
                      }
                    >
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
function ProxyEditor({
  proxy,
  onClose,
  onSaved,
}: {
  proxy?: Proxy;
  onClose: () => void;
  onSaved: () => void;
}) {
  const dialogFocus = useDialogFocus();

  const fieldId = useId();
  const actions = useActions();
  const [value, setValue] = useState<ProxyWrite>({
    name: proxy?.name ?? "",
    protocol: proxy?.protocol ?? "http",
    host: proxy?.host ?? "",
    port: proxy?.port ?? 8080,
    username: proxy?.username ?? "",
    password: null,
  });
  const [passwordAction, setPasswordAction] = useState("keep");
  const update = <K extends keyof ProxyWrite>(key: K, next: ProxyWrite[K]) =>
    setValue((current) => ({ ...current, [key]: next }));
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !actions.running.size) onClose();
      }}
    >
      <DialogContent
        {...dialogFocus}
        showCloseButton={false}
        className="flex max-h-[90dvh] min-h-0 flex-col sm:max-w-3xl"
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (actions.running.size) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (actions.running.size) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{proxy ? "编辑代理" : "添加代理"}</DialogTitle>
          <DialogDescription>{"保存连接信息后，可在列表中执行实际连接检查。"}</DialogDescription>
        </DialogHeader>
        <DialogClose asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="absolute right-4 top-4"
            aria-label="关闭"
            disabled={actions.running.size > 0}
          >
            <X />
          </Button>
        </DialogClose>
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          aria-busy={actions.isBusy("app\\proxies\\page.tsx:form:8")}
          onSubmit={(event) =>
            actions.submit(
              event,
              "app\\proxies\\page.tsx:form:8",
              async () => {
                await request(proxy ? `/proxies/${proxy.id}` : "/proxies", {
                  method: proxy ? "PUT" : "POST",
                  body: {
                    ...value,
                    password:
                      passwordAction === "keep"
                        ? null
                        : passwordAction === "remove"
                          ? ""
                          : value.password,
                  },
                });
                onSaved();
              },
              "已保存",
            )
          }
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={actions.isBusy("app\\proxies\\page.tsx:form:8")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      连接信息
                    </CardTitle>
                  </div>
                  <Field>
                    <FieldLabel
                      htmlFor={fieldId + "-field-9" + "-" + encodeURIComponent(String("名称"))}
                    >
                      {"名称"}
                    </FieldLabel>
                    <Input
                      id={fieldId + "-field-9" + "-" + encodeURIComponent(String("名称"))}
                      aria-label={"名称"}
                      value={value.name}
                      required
                      maxLength={128}
                      onChange={(event) => update("name", event.target.value)}
                    />
                  </Field>
                  <div className="grid sm:grid-cols-2 gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={fieldId + "-field-10" + "-" + encodeURIComponent(String("协议"))}
                      >
                        {"协议"}
                      </FieldLabel>
                      <Select
                        value={value.protocol}
                        onValueChange={(next) =>
                          ((protocol) => update("protocol", protocol as Proxy["protocol"]))(
                            next ===
                              fieldId +
                                "-field-10" +
                                "-" +
                                encodeURIComponent(String("协议")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                      >
                        <SelectTrigger
                          id={fieldId + "-field-10" + "-" + encodeURIComponent(String("协议"))}
                          aria-label={"协议"}
                          data-required={false ? "true" : undefined}
                          data-empty={String(value.protocol) === "" ? "true" : undefined}
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              ["http", "https", "socks5", "socks5h"]
                                .map((value) => ({
                                  value,
                                  label: value.toUpperCase(),
                                }))
                                .find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {["http", "https", "socks5", "socks5h"]
                            .map((value) => ({
                              value,
                              label: value.toUpperCase(),
                            }))
                            .map((option) => (
                              <SelectItem
                                key={option.value}
                                value={
                                  option.value ||
                                  fieldId +
                                    "-field-10" +
                                    "-" +
                                    encodeURIComponent(String("协议")) +
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
                        htmlFor={fieldId + "-field-11" + "-" + encodeURIComponent(String("端口"))}
                      >
                        {"端口"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-11" + "-" + encodeURIComponent(String("端口"))}
                        aria-label={"端口"}
                        value={value.port}
                        type="number"
                        min="1"
                        max="65535"
                        required
                        onChange={(event) => update("port", Number(event.target.value))}
                      />
                    </Field>
                  </div>
                  <Field>
                    <FieldLabel
                      htmlFor={fieldId + "-field-12" + "-" + encodeURIComponent(String("主机"))}
                    >
                      {"主机"}
                    </FieldLabel>
                    <Input
                      id={fieldId + "-field-12" + "-" + encodeURIComponent(String("主机"))}
                      aria-label={"主机"}
                      value={value.host}
                      required
                      onChange={(event) => update("host", event.target.value)}
                      placeholder="代理主机或 IP 地址"
                    />
                  </Field>
                </section>
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      身份验证
                    </CardTitle>
                    <CardDescription>
                      {proxy?.has_password
                        ? "已保存密码，出于安全原因不显示原值。"
                        : "此代理尚未配置密码。"}
                    </CardDescription>
                  </div>
                  <div className="grid sm:grid-cols-2 gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={fieldId + "-field-13" + "-" + encodeURIComponent(String("用户名"))}
                      >
                        {"用户名"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-13" + "-" + encodeURIComponent(String("用户名"))}
                        aria-label={"用户名"}
                        value={value.username ?? ""}
                        autoComplete="off"
                        onChange={(event) => update("username", event.target.value)}
                      />
                    </Field>
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId + "-field-14" + "-" + encodeURIComponent(String("密码操作"))
                        }
                      >
                        {"密码操作"}
                      </FieldLabel>
                      <Select
                        value={passwordAction}
                        onValueChange={(next) =>
                          setPasswordAction(
                            next ===
                              fieldId +
                                "-field-14" +
                                "-" +
                                encodeURIComponent(String("密码操作")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                      >
                        <SelectTrigger
                          id={fieldId + "-field-14" + "-" + encodeURIComponent(String("密码操作"))}
                          aria-label={"密码操作"}
                          data-required={false ? "true" : undefined}
                          data-empty={String(passwordAction) === "" ? "true" : undefined}
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              [
                                {
                                  value: "keep",
                                  label: proxy?.has_password ? "保留已保存密码" : "不设置密码",
                                },
                                {
                                  value: "replace",
                                  label: proxy?.has_password ? "更换密码" : "设置密码",
                                },
                                ...(proxy?.has_password
                                  ? [{ value: "remove", label: "清除已保存密码" }]
                                  : []),
                              ].find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {[
                            {
                              value: "keep",
                              label: proxy?.has_password ? "保留已保存密码" : "不设置密码",
                            },
                            {
                              value: "replace",
                              label: proxy?.has_password ? "更换密码" : "设置密码",
                            },
                            ...(proxy?.has_password
                              ? [{ value: "remove", label: "清除已保存密码" }]
                              : []),
                          ].map((option) => (
                            <SelectItem
                              key={option.value}
                              value={
                                option.value ||
                                fieldId +
                                  "-field-14" +
                                  "-" +
                                  encodeURIComponent(String("密码操作")) +
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
                  </div>
                  {passwordAction === "replace" && (
                    <Field>
                      <FieldLabel
                        htmlFor={fieldId + "-field-15" + "-" + encodeURIComponent(String("新密码"))}
                      >
                        {"新密码"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-15" + "-" + encodeURIComponent(String("新密码"))}
                        aria-label={"新密码"}
                        value={value.password ?? ""}
                        required
                        type="password"
                        autoComplete="new-password"
                        onChange={(event) => update("password", event.target.value)}
                      />
                    </Field>
                  )}
                  {passwordAction === "remove" && (
                    <Alert>
                      <Info />
                      <AlertDescription>保存后将清除该代理的密码。</AlertDescription>
                    </Alert>
                  )}
                </section>
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
            {onClose && (
              <Button
                type="button"
                variant="outline"
                disabled={actions.isBusy("app\\proxies\\page.tsx:form:8")}
                onClick={onClose}
              >
                {"取消"}
              </Button>
            )}
            <Button type="submit" disabled={actions.isBusy("app\\proxies\\page.tsx:form:8")}>
              {actions.isBusy("app\\proxies\\page.tsx:form:8") && <Spinner />}
              {actions.isBusy("app\\proxies\\page.tsx:form:8") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </DialogContent>
    </Dialog>
  );
}
