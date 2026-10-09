"use client";
import { WalletEntryDialog } from "@/app/components/wallet-entries/wallet-entry-dialog";
import { defaults, emptyQuery, useWalletEntriesPage } from "@/app/data/wallet-entries";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
import { toastError, validateForm } from "@/lib/actions";
import { date, money } from "@/lib/format";
import { userOptionLabel, type UserOption } from "@/lib/user-lookup";
import { walletSources } from "@/lib/wallet";

export default function WalletEntriesPage() {
  const {
    id,
    filters,
    setFilters,
    setApplied,
    selectedUser,
    setSelectedUser,
    selected,
    setSelected,
    setPage,
    userLookup,
    resource,
    pagination,
    columns,
  } = useWalletEntriesPage();
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
      <WalletEntryDialog selected={selected} setSelected={setSelected} />
    </>
  );
}
