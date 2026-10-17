"use client";
import { UserEditorDialog } from "@/app/components/users/user-editor-dialog";
import { UserRecords } from "@/app/components/users/user-records";
import { WalletAdjustmentDialog } from "@/app/components/users/wallet-adjustment-dialog";
import { useUsersPage } from "@/app/data/users";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
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
import { money } from "@/lib/format";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  Plus,
  RotateCcw,
  Search,
} from "lucide-react";
import Link from "next/link";

export default function UsersPage() {
  const {
    columns,
    search,
    setSearch,
    setApplied,
    editing,
    setEditing,
    detailId,
    setDetailId,
    adjusting,
    setAdjusting,
    before,
    amount,
    delta,
    after,
    actions,
    focus,
    id,
    resource,
    pagination,
  } = useUsersPage();
  return (
    <>
      <Card size="sm">
        <CardContent className="flex flex-wrap items-center gap-2">
          <form
            className="flex w-full items-center gap-1.5 sm:w-auto sm:gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              setApplied(search.trim());
              resource.reload(1);
            }}
          >
            <Field className="min-w-0 flex-1 sm:w-52 sm:flex-none">
              <FieldLabel className="sr-only" htmlFor={`${id}-search`}>
                搜索用户
              </FieldLabel>
              <Input
                id={`${id}-search`}
                placeholder="搜索用户名、名称或邮箱"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </Field>
            <Button type="submit" size="sm">
              <Search />
              查询
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                setSearch("");
                setApplied("");
                resource.reload(1);
              }}
            >
              <RotateCcw />
              重置
            </Button>
          </form>
          <div className="ml-auto flex flex-wrap items-center gap-1.5">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button type="button" variant="outline" size="sm">
                  <Columns3 />
                  显示列
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>
                  {columns.mobile ? "手机显示列" : "桌面显示列"}
                </DropdownMenuLabel>
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
            <Button
              size="sm"
              disabled={!resource.ready}
              onClick={() =>
                setEditing({
                  id: "",
                  username: "",
                  name: "",
                  email: "",
                  password: "",
                  enabled: true,
                  wallet_balance_usd: "0",
                  revision: 0,
                  created_at: "",
                })
              }
            >
              <Plus />
              创建用户
            </Button>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead hidden={!columns.isVisible("用户名")}>用户名</TableHead>
                <TableHead hidden={!columns.isVisible("名称")}>名称</TableHead>
                <TableHead hidden={!columns.isVisible("邮箱")}>邮箱</TableHead>
                <TableHead hidden={!columns.isVisible("钱包（USD）")}>钱包（USD）</TableHead>
                <TableHead hidden={!columns.isVisible("状态")}>状态</TableHead>
                <TableHead hidden={!columns.isVisible("操作")}>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {pagination.rows.map((user) => (
                <TableRow key={user.id}>
                  <TableCell hidden={!columns.isVisible("用户名")} className="font-medium">
                    {user.username}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("名称")}>{user.name}</TableCell>
                  <TableCell hidden={!columns.isVisible("邮箱")}>{user.email}</TableCell>
                  <TableCell hidden={!columns.isVisible("钱包（USD）")} className="tabular-nums">
                    {money(user.wallet_balance_usd)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("状态")}>
                    <Badge variant={user.enabled ? "secondary" : "outline"}>
                      {user.enabled ? "启用" : "停用"}
                    </Badge>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作")}>
                    <div className="flex flex-wrap gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={!resource.ready}
                        onClick={() => setEditing({ ...user, password: "" })}
                      >
                        编辑
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={!resource.ready}
                        onClick={() =>
                          setAdjusting({
                            user,
                            request_id: crypto.randomUUID(),
                            direction: "increase",
                            amount: "",
                            reason: "",
                          })
                        }
                      >
                        调整余额
                      </Button>
                      <Button size="sm" variant="outline" onClick={() => setDetailId(user.id)}>
                        记录
                      </Button>
                      <Button size="sm" variant="outline" asChild>
                        <Link href={`/subscriptions/?user_id=${encodeURIComponent(user.id)}`}>
                          订阅
                        </Link>
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {pagination.rows.length === 0 && (
                <TableRow>
                  <TableCell colSpan={columns.count} className="text-center text-muted-foreground">
                    暂无用户
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="用户分页">
            <PaginationContent className="flex-wrap justify-end gap-1">
              <PaginationItem>
                <Select {...pagination.size}>
                  <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper" side="bottom" align="end">
                    {[10, 20, 30, 50].map((n) => (
                      <SelectItem value={String(n)} key={n}>
                        {n} 条/页
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
      <WalletAdjustmentDialog
        adjusting={adjusting}
        actions={actions}
        setAdjusting={setAdjusting}
        resource={resource}
        amount={amount}
        delta={delta}
        after={after}
        before={before}
        id={id}
      />
      <UserEditorDialog
        editing={editing}
        actions={actions}
        setEditing={setEditing}
        focus={focus}
        resource={resource}
        id={id}
      />
      {detailId && <UserRecords id={detailId} close={() => setDetailId(undefined)} />}
    </>
  );
}
