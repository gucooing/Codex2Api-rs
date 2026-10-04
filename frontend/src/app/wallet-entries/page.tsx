"use client";
import { useId, useState } from "react";
import { query } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { toastError, useErrorToast, validateForm } from "@/lib/actions";
import { usePageControls } from "@/lib/pagination";
import { useColumnVisibility } from "@/lib/columns";
import { useUserLookup, userOptionLabel, type UserOption } from "@/lib/user-lookup";
import { walletSources } from "@/lib/wallet";
import { date, money } from "@/lib/format";
import type { AdminWalletEntry, WalletEntryPage } from "@/lib/users";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  Combobox,
  ComboboxInput,
  ComboboxContent,
  ComboboxList,
  ComboboxItem,
  ComboboxEmpty,
} from "@/components/ui/combobox";
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
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuCheckboxItem,
  DropdownMenuSeparator,
  DropdownMenuItem,
} from "@/components/ui/dropdown-menu";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";

const defaults = { user_id: "", kind: "all", from: "", until: "" };
type Applied = { user_id: string; kind: string; from_ms: number | null; until_ms: number | null };
const emptyQuery: Applied = { user_id: "", kind: "", from_ms: null, until_ms: null };
export default function WalletEntriesPage() {
  const id = useId();
  const [filters, setFilters] = useState(defaults);
  const [applied, setApplied] = useState(emptyQuery);
  const [selectedUser, setSelectedUser] = useState<UserOption | null>(null);
  const [selected, setSelected] = useState<AdminWalletEntry>();
  const [page, setPage] = useState(1);
  const [size, setSize] = useState(20);
  const userLookup = useUserLookup();
  const resource = useResource<WalletEntryPage>(
    `/wallet-entries${query({ ...applied, page, limit: size })}`,
  );
  const pagination = usePageControls(
    page,
    resource.data?.total,
    setPage,
    size,
    !resource.data,
    setSize,
  );
  const columns = useColumnVisibility(
    "wallet-entries",
    ["时间", "用户", "来源", "变动", "前余额", "后余额", "操作人", "原因", "操作"],
    ["用户", "变动", "后余额", "操作"],
  );
  useErrorToast(resource.error);
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <form
          aria-label="流水筛选"
          noValidate
          className="flex flex-wrap items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (!validateForm(event.currentTarget)) return;
            const from = filters.from ? Date.parse(filters.from) : null;
            const until = filters.until ? Date.parse(filters.until) : null;
            if (
              (from !== null && !Number.isFinite(from)) ||
              (until !== null && !Number.isFinite(until)) ||
              (from !== null && until !== null && from >= until)
            ) {
              toastError("请选择有效的起止时间");
              return;
            }
            setApplied({
              user_id: filters.user_id,
              kind: filters.kind === "all" ? "" : filters.kind,
              from_ms: from,
              until_ms: until,
            });
            setPage(1);
            resource.reload();
          }}
        >
          <Field className="w-48">
            <FieldLabel className="sr-only" htmlFor={`${id}-user`}>
              用户
            </FieldLabel>
            <Combobox<UserOption>
              items={userLookup.data?.items ?? []}
              value={selectedUser}
              onValueChange={(item) => {
                setSelectedUser(item);
                setFilters((v) => ({ ...v, user_id: item?.id ?? "" }));
                userLookup.setSearch("");
              }}
              itemToStringLabel={userOptionLabel}
              itemToStringValue={(item) => item.id}
              isItemEqualToValue={(item, value) => item.id === value.id}
              filter={null}
              open={userLookup.open}
              onOpenChange={(open, details) => {
                userLookup.setOpen(open);
                if (open && details.reason !== "input-change") userLookup.setSearch("");
              }}
              onInputValueChange={(text, details) => {
                if (details.reason === "input-change") {
                  userLookup.setSearch(text);
                  if (!text) {
                    setSelectedUser(null);
                    setFilters((v) => ({ ...v, user_id: "" }));
                  }
                }
              }}
            >
              <ComboboxInput
                id={`${id}-user`}
                placeholder="搜索选择用户"
                showClear
                maxLength={128}
                className="w-full"
              />
              <ComboboxContent>
                <ComboboxEmpty>
                  {userLookup.loading ? "加载中…" : userLookup.error ? "加载失败" : "没有匹配用户"}
                </ComboboxEmpty>
                <ComboboxList aria-busy={userLookup.loading}>
                  {(item: UserOption) => (
                    <ComboboxItem key={item.id} value={item}>
                      {userOptionLabel(item)}
                    </ComboboxItem>
                  )}
                </ComboboxList>
              </ComboboxContent>
            </Combobox>
          </Field>
          <Field className="w-32">
            <FieldLabel className="sr-only" htmlFor={`${id}-kind`}>
              来源
            </FieldLabel>
            <Select
              value={filters.kind}
              onValueChange={(kind) => setFilters((v) => ({ ...v, kind }))}
            >
              <SelectTrigger id={`${id}-kind`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="all">全部来源</SelectItem>
                {Object.entries(walletSources).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field orientation="horizontal" className="w-auto">
            <FieldLabel htmlFor={`${id}-from`}>开始</FieldLabel>
            <Input
              id={`${id}-from`}
              aria-label="开始时间"
              type="datetime-local"
              className="w-48"
              value={filters.from}
              onChange={(event) => setFilters((v) => ({ ...v, from: event.target.value }))}
            />
          </Field>
          <Field orientation="horizontal" className="w-auto">
            <FieldLabel htmlFor={`${id}-until`}>结束</FieldLabel>
            <Input
              id={`${id}-until`}
              aria-label="结束时间（不含）"
              type="datetime-local"
              className="w-48"
              value={filters.until}
              onChange={(event) => setFilters((v) => ({ ...v, until: event.target.value }))}
            />
          </Field>
          <Button type="submit">查询</Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              setFilters(defaults);
              setApplied(emptyQuery);
              setSelectedUser(null);
              userLookup.setSearch("");
              setPage(1);
              resource.reload();
            }}
          >
            重置
          </Button>
        </form>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline">显示列</Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel>{columns.mobile ? "手机显示列" : "桌面显示列"}</DropdownMenuLabel>
            {columns.labels.map((label) => (
              <DropdownMenuCheckboxItem
                key={label}
                checked={columns.isVisible(label)}
                disabled={columns.count === 1 && columns.isVisible(label)}
                onSelect={(event) => event.preventDefault()}
                onCheckedChange={(checked) => columns.setVisible(label, checked === true)}
              >
                {label}
              </DropdownMenuCheckboxItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={columns.showAll}>显示全部列</DropdownMenuItem>
            <DropdownMenuItem onSelect={columns.reset}>恢复默认列</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead hidden={!columns.isVisible("时间")}>时间</TableHead>
                <TableHead hidden={!columns.isVisible("用户")}>用户</TableHead>
                <TableHead hidden={!columns.isVisible("来源")}>来源</TableHead>
                <TableHead hidden={!columns.isVisible("变动")}>变动（USD）</TableHead>
                <TableHead hidden={!columns.isVisible("前余额")}>前余额（USD）</TableHead>
                <TableHead hidden={!columns.isVisible("后余额")}>后余额（USD）</TableHead>
                <TableHead hidden={!columns.isVisible("操作人")}>操作人</TableHead>
                <TableHead hidden={!columns.isVisible("原因")}>原因</TableHead>
                <TableHead hidden={!columns.isVisible("操作")}>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {resource.data?.items.map((entry) => (
                <TableRow key={entry.id}>
                  <TableCell hidden={!columns.isVisible("时间")}>
                    {date(entry.created_at)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("用户")}>{entry.username}</TableCell>
                  <TableCell hidden={!columns.isVisible("来源")}>
                    <Badge variant="secondary">{walletSources[entry.kind]}</Badge>
                    {entry.plan_name && (
                      <p className="mt-1 text-xs text-muted-foreground">{entry.plan_name}</p>
                    )}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("变动")} className="tabular-nums">
                    {entry.amount_cents > 0 ? "+" : ""}
                    {money(entry.amount_cents / 100)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("前余额")} className="tabular-nums">
                    {money(entry.balance_before_cents / 100)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("后余额")} className="tabular-nums">
                    {money(entry.balance_cents / 100)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作人")}>
                    {entry.operator_name ?? "—"}
                  </TableCell>
                  <TableCell
                    hidden={!columns.isVisible("原因")}
                    className="max-w-48 truncate"
                    title={entry.reason ?? undefined}
                  >
                    {entry.reason ?? "—"}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作")}>
                    <Button size="sm" variant="outline" onClick={() => setSelected(entry)}>
                      详情
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {resource.data?.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={columns.count}>暂无流水</TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end">
            <PaginationContent className="flex-wrap">
              <PaginationItem>
                共 {resource.data?.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
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
      <Dialog
        open={!!selected}
        onOpenChange={(open) => {
          if (!open) setSelected(undefined);
        }}
      >
        <DialogContent className="flex max-h-[90dvh] flex-col">
          <DialogHeader>
            <DialogTitle>流水详情</DialogTitle>
            <DialogDescription>{selected?.username ?? "钱包流水"}</DialogDescription>
          </DialogHeader>
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 pr-3 text-sm">
              <dt className="text-muted-foreground">流水号</dt>
              <dd className="break-all">{selected?.id ?? "—"}</dd>
              <dt className="text-muted-foreground">用户</dt>
              <dd>{selected ? `${selected.username} · ${selected.user_name}` : "—"}</dd>
              <dt className="text-muted-foreground">来源</dt>
              <dd>{selected ? walletSources[selected.kind] : "—"}</dd>
              <dt className="text-muted-foreground">套餐</dt>
              <dd>{selected?.plan_name ?? "—"}</dd>
              <dt className="text-muted-foreground">关联订单</dt>
              <dd className="break-all">{selected?.order_id ?? "—"}</dd>
              <dt className="text-muted-foreground">前余额</dt>
              <dd>{selected ? money(selected.balance_before_cents / 100) : "—"} USD</dd>
              <dt className="text-muted-foreground">变动金额</dt>
              <dd>
                {selected
                  ? `${selected.amount_cents > 0 ? "+" : ""}${money(selected.amount_cents / 100)}`
                  : "—"}{" "}
                USD
              </dd>
              <dt className="text-muted-foreground">后余额</dt>
              <dd>{selected ? money(selected.balance_cents / 100) : "—"} USD</dd>
              <dt className="text-muted-foreground">时间</dt>
              <dd>{selected ? date(selected.created_at) : "—"}</dd>
              <dt className="text-muted-foreground">操作人</dt>
              <dd>{selected?.operator_name ?? "—"}</dd>
              <dt className="text-muted-foreground">原因</dt>
              <dd className="whitespace-pre-wrap break-words">{selected?.reason ?? "—"}</dd>
            </dl>
          </ScrollArea>
          <DialogFooter>
            <Button variant="outline" onClick={() => setSelected(undefined)}>
              关闭
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
