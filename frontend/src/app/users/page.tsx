"use client";
import { useColumnVisibility } from "@/lib/columns";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuCheckboxItem,
  DropdownMenuSeparator,
  DropdownMenuItem,
} from "@/components/ui/dropdown-menu";

import { useId, useState } from "react";
import Link from "next/link";
import { request, type List } from "@/lib/api";
import type { User, UserDetail } from "@/lib/users";
import { useResource } from "@/lib/hooks";
import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import { useTablePagination } from "@/lib/pagination";
import { date, money } from "@/lib/format";
import { usdCents } from "@/lib/wallet";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldSet, FieldGroup } from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableHeader,
  TableHead,
  TableBody,
  TableRow,
  TableCell,
} from "@/components/ui/table";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";

export default function UsersPage() {
  const columns = useColumnVisibility(
    "users",
    ["用户名", "名称", "邮箱", "钱包（USD）", "状态", "操作"],
    ["用户名", "钱包（USD）", "操作"],
  );
  const resource = useResource<List<User>>("/users");
  const [search, setSearch] = useState("");
  const [applied, setApplied] = useState("");
  const [editing, setEditing] = useState<User & { password: string }>();
  const [detailId, setDetailId] = useState<string>();
  const [adjusting, setAdjusting] = useState<{
    user: User;
    request_id: string;
    direction: "increase" | "decrease";
    amount: string;
    reason: string;
  }>();
  const before = adjusting ? usdCents(adjusting.user.wallet_balance_usd) : null;
  const amount = adjusting ? usdCents(adjusting.amount) : null;
  const delta = amount === null ? null : amount * (adjusting?.direction === "decrease" ? -1 : 1);
  const after = before === null || delta === null ? null : before + delta;

  const actions = useActions();
  const focus = useDialogFocus();
  const id = useId();
  const rows =
    resource.data?.items.filter((u) =>
      `${u.username} ${u.name} ${u.email}`.toLowerCase().includes(applied.toLowerCase()),
    ) ?? [];
  const pagination = useTablePagination(rows, applied, !!resource.data);
  useErrorToast(resource.error);
  return (
    <>
      <Card>
        <CardContent className="flex flex-wrap items-end gap-3">
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              setApplied(search.trim());
              resource.reload();
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
                resource.reload();
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
      <Dialog
        open={!!adjusting}
        onOpenChange={(open) => {
          if (!open && !actions.isBusy("adjust-wallet")) setAdjusting(undefined);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>调整余额</DialogTitle>
            <DialogDescription>{adjusting?.user.username ?? "用户钱包"}</DialogDescription>
          </DialogHeader>
          <form
            noValidate
            onSubmit={(event) =>
              actions.submit(
                event,
                "adjust-wallet",
                async () => {
                  if (
                    !adjusting ||
                    !resource.ready ||
                    !amount ||
                    delta === null ||
                    after === null ||
                    !Number.isSafeInteger(after) ||
                    after < 0
                  )
                    throw new Error("请输入有效金额，减少金额不能超过余额");
                  await request(
                    `/users/${encodeURIComponent(adjusting.user.id)}/wallet-adjustments`,
                    {
                      method: "POST",
                      body: {
                        request_id: adjusting.request_id,
                        amount_cents: delta,
                        expected_revision: adjusting.user.revision,
                        reason: adjusting.reason.trim() || null,
                      },
                    },
                  );
                  setAdjusting(undefined);
                  resource.reload();
                },
                "余额已调整",
              )
            }
          >
            <FieldSet disabled={actions.isBusy("adjust-wallet")}>
              <FieldGroup>
                <div className="grid grid-cols-2 gap-3 text-sm">
                  <p>
                    当前余额{" "}
                    <span className="font-medium">
                      {before === null ? "—" : money(before / 100)} USD
                    </span>
                  </p>
                  <p>
                    调整后{" "}
                    <span className="font-medium">
                      {after === null || !Number.isSafeInteger(after) || after < 0
                        ? "—"
                        : money(after / 100)}{" "}
                      USD
                    </span>
                  </p>
                </div>
                <Field>
                  <FieldLabel htmlFor={`${id}-wallet-direction`}>操作</FieldLabel>
                  <Select
                    value={adjusting?.direction ?? "increase"}
                    onValueChange={(direction) =>
                      setAdjusting((v) =>
                        v ? { ...v, direction: direction as "increase" | "decrease" } : v,
                      )
                    }
                  >
                    <SelectTrigger id={`${id}-wallet-direction`}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="increase">增加余额</SelectItem>
                      <SelectItem value="decrease">减少余额</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-wallet-amount`}>金额（USD）</FieldLabel>
                  <Input
                    id={`${id}-wallet-amount`}
                    inputMode="decimal"
                    value={adjusting?.amount ?? ""}
                    onChange={(e) =>
                      setAdjusting((v) => (v ? { ...v, amount: e.target.value } : v))
                    }
                    required
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-wallet-reason`}>调整原因（选填）</FieldLabel>
                  <Input
                    id={`${id}-wallet-reason`}
                    value={adjusting?.reason ?? ""}
                    onChange={(e) =>
                      setAdjusting((v) => (v ? { ...v, reason: e.target.value } : v))
                    }
                    maxLength={300}
                  />
                </Field>
                <DialogFooter>
                  <Button type="button" variant="outline" onClick={() => setAdjusting(undefined)}>
                    取消
                  </Button>
                  <Button type="submit" disabled={!resource.ready || !adjusting}>
                    确认{adjusting?.direction === "decrease" ? "减少" : "增加"}
                  </Button>
                </DialogFooter>
              </FieldGroup>
            </FieldSet>
          </form>
        </DialogContent>
      </Dialog>
      <Dialog
        open={!!editing}
        onOpenChange={(open) => {
          if (!open && !actions.isBusy("save-user")) setEditing(undefined);
        }}
      >
        <DialogContent {...focus}>
          <DialogHeader>
            <DialogTitle>{editing?.id ? "编辑用户" : "创建用户"}</DialogTitle>
            <DialogDescription>
              停用或修改密码会撤销用户网页会话及所有平台设备登录。
            </DialogDescription>
          </DialogHeader>
          <form
            noValidate
            onSubmit={(e) =>
              actions.submit(e, "save-user", async () => {
                if (!editing || !resource.ready) throw new Error("请先加载用户资料");
                await request(editing.id ? `/users/${editing.id}` : "/users", {
                  method: editing.id ? "PUT" : "POST",
                  body: {
                    username: editing.username,
                    name: editing.name,
                    email: editing.email,
                    password: editing.password,
                    enabled: editing.enabled,
                    revision: editing.id ? editing.revision : null,
                  },
                });
                setEditing(undefined);
                resource.reload();
              })
            }
          >
            <FieldSet disabled={!editing || !resource.ready || actions.isBusy("save-user")}>
              <FieldGroup>
                {(
                  [
                    ["username", "用户名"],
                    ["name", "名称"],
                    ["email", "邮箱"],
                    ["password", editing?.id ? "新密码（留空保留）" : "密码"],
                  ] as const
                ).map(([key, label]) => (
                  <Field key={key}>
                    <FieldLabel htmlFor={`${id}-${key}`}>{label}</FieldLabel>
                    <Input
                      id={`${id}-${key}`}
                      type={key === "password" ? "password" : key === "email" ? "email" : "text"}
                      autoComplete={key === "password" ? "new-password" : "off"}
                      required={key !== "password" || !editing?.id}
                      value={editing?.[key] ?? ""}
                      onChange={(e) => setEditing((v) => (v ? { ...v, [key]: e.target.value } : v))}
                    />
                  </Field>
                ))}
                <Field orientation="horizontal">
                  <FieldLabel htmlFor={`${id}-enabled`}>允许登录</FieldLabel>
                  <Switch
                    id={`${id}-enabled`}
                    checked={editing?.enabled ?? false}
                    onCheckedChange={(enabled) => setEditing((v) => (v ? { ...v, enabled } : v))}
                  />
                </Field>
                <DialogFooter>
                  <Button type="submit">保存</Button>
                </DialogFooter>
              </FieldGroup>
            </FieldSet>
          </form>
        </DialogContent>
      </Dialog>
      {detailId && <UserRecords id={detailId} close={() => setDetailId(undefined)} />}
    </>
  );
}
function UserRecords({ id, close }: { id: string; close: () => void }) {
  const resource = useResource<UserDetail>(`/users/${encodeURIComponent(id)}`);
  const pagination = useTablePagination(resource.data?.wallet_entries ?? [], id, !!resource.data);
  useErrorToast(resource.error);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{resource.data?.user.name ?? "用户记录"}</DialogTitle>
          <DialogDescription>钱包与实际订阅记录。</DialogDescription>
        </DialogHeader>
        <Tabs defaultValue="wallet">
          <TabsList>
            <TabsTrigger value="wallet">钱包记录</TabsTrigger>
            <TabsTrigger value="subscriptions">订阅</TabsTrigger>
          </TabsList>
          <TabsContent value="wallet">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>时间</TableHead>
                  <TableHead>操作</TableHead>
                  <TableHead>变动前</TableHead>
                  <TableHead>金额</TableHead>
                  <TableHead>结余</TableHead>
                  <TableHead>操作人</TableHead>
                  <TableHead>原因</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {pagination.rows.map((e) => (
                  <TableRow key={e.id}>
                    <TableCell>{date(e.created_at)}</TableCell>
                    <TableCell>
                      {e.kind === "system_adjustment" ? "系统操作" : e.plan_name}
                    </TableCell>
                    <TableCell>{money(e.balance_before_cents / 100)}</TableCell>
                    <TableCell>
                      {e.amount_cents > 0 ? "+" : ""}
                      {money(e.amount_cents / 100)}
                    </TableCell>
                    <TableCell>{money(e.balance_cents / 100)}</TableCell>
                    <TableCell>{e.operator_name ?? "—"}</TableCell>
                    <TableCell className="max-w-64 truncate" title={e.reason ?? undefined}>
                      {e.reason ?? "—"}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <Pagination>
              <PaginationContent className="flex-wrap">
                <PaginationItem>共 {pagination.total ?? "—"} 条</PaginationItem>
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
          </TabsContent>
          <TabsContent value="subscriptions">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>平台</TableHead>
                  <TableHead>套餐</TableHead>
                  <TableHead>到期时间</TableHead>
                  <TableHead>操作</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {resource.data?.subscriptions.map((s) => (
                  <TableRow key={s.virtual_account_id}>
                    <TableCell>{s.provider_id}</TableCell>
                    <TableCell>{s.plan_name}</TableCell>
                    <TableCell>
                      {s.subscription_expires_at ? date(s.subscription_expires_at) : "长期有效"}
                    </TableCell>
                    <TableCell>
                      <Button variant="outline" asChild>
                        <Link
                          href={`/consumers/detail/?id=${encodeURIComponent(s.virtual_account_id)}`}
                        >
                          配置与记录
                        </Link>
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
