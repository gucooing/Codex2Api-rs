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

import { Suspense, useId, useState } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { request, query, type List, type Plan } from "@/lib/api";
import type { User, UserSubscription } from "@/lib/users";
import { useResource } from "@/lib/hooks";
import {
  Combobox,
  ComboboxInput,
  ComboboxContent,
  ComboboxList,
  ComboboxItem,
  ComboboxEmpty,
} from "@/components/ui/combobox";
import { useUserLookup, userOptionLabel, type UserOption } from "@/lib/user-lookup";

import { useActions, useDialogFocus, useErrorToast } from "@/lib/actions";
import { useTablePagination } from "@/lib/pagination";
import { localDate } from "@/lib/domain";
import { date } from "@/lib/format";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldSet, FieldGroup, FieldDescription } from "@/components/ui/field";
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

export default function Page() {
  return (
    <Suspense>
      <SubscriptionRoute />
    </Suspense>
  );
}
type Edit = {
  reissue: boolean;
  id: string;
  user_id: string;
  provider_id: string;
  plan_id: string;
  expires_at: string;
  enabled: boolean;
  revision: number | null;
};
function SubscriptionRoute() {
  const user = useSearchParams().get("user_id") ?? "";
  return <Subscriptions key={user} initialUserId={user} />;
}
const emptyFilters = { user_id: "", plan_id: "all", include_expired: false };
function Subscriptions({ initialUserId }: { initialUserId: string }) {
  const columns = useColumnVisibility(
    "subscriptions",
    ["用户", "平台", "套餐", "到期时间", "状态", "操作"],
    ["用户", "套餐", "操作"],
  );
  const [filters, setFilters] = useState({ ...emptyFilters, user_id: initialUserId });
  const [applied, setApplied] = useState({ ...emptyFilters, user_id: initialUserId });
  const [chosenUser, setChosenUser] = useState<UserOption | null>(
    initialUserId ? { id: initialUserId, username: "", name: "已选用户" } : null,
  );
  const userLookup = useUserLookup();
  const resource = useResource<List<UserSubscription>>(
    `/subscriptions${query({ user_id: applied.user_id, plan_id: applied.plan_id === "all" ? "" : applied.plan_id, include_expired: applied.include_expired })}`,
  );
  const users = useResource<List<User>>("/users");
  const plans = useResource<List<Plan>>("/plans");
  const selectedUser = users.data?.items.find((user) => user.id === chosenUser?.id) ?? chosenUser;
  const [editing, setEditing] = useState<Edit>();
  const actions = useActions();
  const focus = useDialogFocus();
  const id = useId();
  const pagination = useTablePagination(
    resource.data?.items ?? [],
    JSON.stringify(applied),
    !!resource.data,
  );
  useErrorToast(resource.error);
  useErrorToast(users.error);
  useErrorToast(plans.error);
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <form
          aria-label="订阅筛选"
          noValidate
          className="flex flex-wrap items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            setApplied(filters);
            pagination.first.onClick();
            resource.reload();
          }}
        >
          <Field className="w-48">
            <FieldLabel className="sr-only" htmlFor={`${id}-filter-user`}>
              用户
            </FieldLabel>
            <Combobox<UserOption>
              items={userLookup.data?.items ?? []}
              value={selectedUser}
              onValueChange={(item) => {
                setChosenUser(item);
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
                    setChosenUser(null);
                    setFilters((v) => ({ ...v, user_id: "" }));
                  }
                }
              }}
            >
              <ComboboxInput
                id={`${id}-filter-user`}
                placeholder="搜索选择用户"
                showClear
                className="w-full"
                maxLength={128}
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
          <Field className="w-40">
            <FieldLabel className="sr-only" htmlFor={`${id}-filter-plan`}>
              套餐
            </FieldLabel>
            <Select
              value={filters.plan_id}
              onValueChange={(plan_id) => setFilters((v) => ({ ...v, plan_id }))}
            >
              <SelectTrigger id={`${id}-filter-plan`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="all">全部套餐</SelectItem>
                {plans.data?.items.map((plan) => (
                  <SelectItem key={plan.id} value={plan.id}>
                    {plan.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field orientation="horizontal" className="w-auto">
            <Switch
              id={`${id}-expired`}
              checked={filters.include_expired}
              onCheckedChange={(include_expired) => setFilters((v) => ({ ...v, include_expired }))}
            />
            <FieldLabel htmlFor={`${id}-expired`}>显示已到期</FieldLabel>
          </Field>
          <Button type="submit">查询</Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              setFilters(emptyFilters);
              setApplied(emptyFilters);
              setChosenUser(null);
              userLookup.setSearch("");
              pagination.first.onClick();
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
          disabled={!resource.ready || !users.ready || !plans.ready}
          onClick={() =>
            setEditing({
              reissue: false,
              id: "",
              user_id: filters.user_id,
              provider_id: "",
              plan_id: "",
              expires_at: localDate(new Date(Date.now() + 30 * 86400000).toISOString()),
              enabled: true,
              revision: null,
            })
          }
        >
          发放订阅
        </Button>
      </div>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead hidden={!columns.isVisible("用户")}>用户</TableHead>
                <TableHead hidden={!columns.isVisible("平台")}>平台</TableHead>
                <TableHead hidden={!columns.isVisible("套餐")}>套餐</TableHead>
                <TableHead hidden={!columns.isVisible("到期时间")}>到期时间</TableHead>
                <TableHead hidden={!columns.isVisible("状态")}>状态</TableHead>
                <TableHead hidden={!columns.isVisible("操作")}>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {pagination.rows.map((s) => (
                <TableRow key={s.virtual_account_id}>
                  <TableCell hidden={!columns.isVisible("用户")}>
                    {s.name}（{s.username}）
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("平台")}>{s.provider_id}</TableCell>
                  <TableCell hidden={!columns.isVisible("套餐")}>{s.plan_name}</TableCell>
                  <TableCell hidden={!columns.isVisible("到期时间")}>
                    {s.subscription_expires_at ? date(s.subscription_expires_at) : "长期有效"}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("状态")}>
                    {!s.enabled ? "停用" : s.expired ? "已到期" : "有效"}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作")}>
                    <div className="flex gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={!resource.ready}
                        onClick={() =>
                          setEditing({
                            reissue: false,
                            id: s.virtual_account_id,
                            user_id: s.user_id,
                            provider_id: s.provider_id,
                            plan_id: s.plan_id,
                            expires_at: localDate(s.subscription_expires_at),
                            enabled: s.enabled,
                            revision: s.revision,
                          })
                        }
                      >
                        调整订阅
                      </Button>
                      <Button size="sm" variant="outline" asChild>
                        <Link
                          href={`/consumers/detail/?id=${encodeURIComponent(s.virtual_account_id)}`}
                        >
                          配置与记录
                        </Link>
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end">
            <PaginationContent className="flex-wrap">
              <PaginationItem>
                共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
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
        open={!!editing}
        onOpenChange={(open) => {
          if (!open && !actions.isBusy("subscription")) setEditing(undefined);
        }}
      >
        <DialogContent {...focus} className="max-h-[90dvh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>{editing?.id ? "调整订阅" : "发放订阅"}</DialogTitle>
            <DialogDescription>
              每位用户在每个平台只有一个订阅。管理员发放不扣钱包余额。
            </DialogDescription>
          </DialogHeader>
          <form
            noValidate
            onSubmit={(e) =>
              actions.submit(e, "subscription", async () => {
                if (!editing || !users.ready || !plans.ready || !resource.ready)
                  throw new Error("请先加载订阅资料");
                if (!editing.user_id || !editing.plan_id) throw new Error("请选择用户和套餐");
                const plan = plans.data?.items.find((p) => p.id === editing.plan_id);
                const existing = resource.data?.items.find(
                  (s) => s.user_id === editing.user_id && s.provider_id === plan?.provider_id,
                );
                const target = editing.id || existing?.virtual_account_id;
                await request(target ? `/subscriptions/${target}` : "/subscriptions", {
                  method: target ? "PUT" : "POST",
                  body: {
                    user_id: editing.user_id,
                    plan_id: editing.plan_id,
                    expires_at: editing.expires_at
                      ? new Date(editing.expires_at).toISOString()
                      : null,
                    enabled: editing.enabled,
                    reissue: editing.reissue,
                    revision: editing.revision ?? existing?.revision ?? null,
                  },
                });
                setEditing(undefined);
                resource.reload();
              })
            }
          >
            <FieldSet
              disabled={
                !editing ||
                !users.ready ||
                !plans.ready ||
                !resource.ready ||
                actions.isBusy("subscription")
              }
            >
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor={`${id}-user`}>用户</FieldLabel>
                  <Select
                    value={editing?.user_id ?? ""}
                    disabled={!!editing?.id}
                    onValueChange={(user_id) => setEditing((v) => (v ? { ...v, user_id } : v))}
                  >
                    <SelectTrigger id={`${id}-user`}>
                      <SelectValue placeholder="请选择用户" />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      {users.data?.items.map((u) => (
                        <SelectItem key={u.id} value={u.id}>
                          {u.username} · {u.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-plan`}>套餐</FieldLabel>
                  <Select
                    value={editing?.plan_id ?? ""}
                    onValueChange={(plan_id) => setEditing((v) => (v ? { ...v, plan_id } : v))}
                  >
                    <SelectTrigger id={`${id}-plan`}>
                      <SelectValue placeholder="请选择套餐" />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      {plans.data?.items
                        .filter((p) => !editing?.id || p.provider_id === editing.provider_id)
                        .map((p) => (
                          <SelectItem key={p.id} value={p.id}>
                            {p.name} · {p.provider_id}
                          </SelectItem>
                        ))}
                    </SelectContent>
                  </Select>
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-expiry`}>到期时间（留空长期有效）</FieldLabel>
                  <Input
                    id={`${id}-expiry`}
                    type="datetime-local"
                    value={editing?.expires_at ?? ""}
                    onChange={(e) =>
                      setEditing((v) => (v ? { ...v, expires_at: e.target.value } : v))
                    }
                  />
                </Field>
                <Field orientation="horizontal">
                  <Switch
                    id={`${id}-reissue`}
                    checked={editing?.reissue ?? false}
                    onCheckedChange={(reissue) =>
                      setEditing((value) => (value ? { ...value, reissue } : value))
                    }
                  />
                  <FieldLabel htmlFor={`${id}-reissue`}>重新发放，使用当前售价</FieldLabel>
                </Field>
                <FieldDescription>
                  重新发放会重启用量周期并保存当前售价作为升级抵扣基准，到期时间按此表单设置。不勾选时保留原有计价记录。
                </FieldDescription>
                <Field orientation="horizontal">
                  <Switch
                    id={`${id}-enabled`}
                    checked={editing?.enabled ?? false}
                    onCheckedChange={(enabled) => setEditing((v) => (v ? { ...v, enabled } : v))}
                  />
                  <FieldLabel htmlFor={`${id}-enabled`}>允许平台登录</FieldLabel>
                </Field>
                <DialogFooter>
                  <Button type="submit">保存</Button>
                </DialogFooter>
              </FieldGroup>
            </FieldSet>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}
