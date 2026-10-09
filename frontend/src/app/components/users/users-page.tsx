"use client";
import { UserEditorDialog } from "@/app/components/users/user-editor-dialog";
import { UserRecords } from "@/app/components/users/user-records";
import { WalletAdjustmentDialog } from "@/app/components/users/wallet-adjustment-dialog";
import { useUsersPage } from "@/app/data/users";
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
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              setApplied(search.trim());
              resource.reload(1);
            }}
          >
            <Field className="w-56">
              <FieldLabel htmlFor={`${id}-search`}>搜索用户</FieldLabel>
              <Input
                id={`${id}-search`}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </Field>
            <Button type="submit">查询</Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setSearch("");
                setApplied("");
                resource.reload(1);
              }}
            >
              重置
            </Button>
          </form>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" variant="outline">
                显示列
              </Button>
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
          <Button
            className="ml-auto"
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
            创建用户
          </Button>
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
                  <TableCell hidden={!columns.isVisible("用户名")}>{user.username}</TableCell>
                  <TableCell hidden={!columns.isVisible("名称")}>{user.name}</TableCell>
                  <TableCell hidden={!columns.isVisible("邮箱")}>{user.email}</TableCell>
                  <TableCell hidden={!columns.isVisible("钱包（USD）")}>
                    {money(user.wallet_balance_usd)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("状态")}>
                    {user.enabled ? "启用" : "停用"}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作")}>
                    <div className="flex gap-2">
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
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="用户分页">
            <PaginationContent className="flex-wrap">
              <PaginationItem>
                <span className="text-sm text-muted-foreground">
                  共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
                </span>
              </PaginationItem>
              <PaginationItem>
                <Select {...pagination.size}>
                  <SelectTrigger aria-label="每页条数">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent side="bottom">
                    {[10, 20, 30, 50].map((n) => (
                      <SelectItem value={String(n)} key={n}>
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
