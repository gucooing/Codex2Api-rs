"use client";
import { SubscriptionEditorDialog } from "@/app/components/subscriptions/subscription-editor-dialog";
import { emptyFilters, useSubscriptions } from "@/app/data/subscriptions";
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
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { localDate } from "@/lib/domain";
import { date } from "@/lib/format";
import { userOptionLabel, type UserOption } from "@/lib/user-lookup";
import Link from "next/link";
import { useSearchParams } from "next/navigation";

export function SubscriptionRoute() {
  const user = useSearchParams().get("user_id") ?? "";
  return <Subscriptions key={user} initialUserId={user} />;
}

export function Subscriptions({ initialUserId }: { initialUserId: string }) {
  const {
    columns,
    filters,
    setFilters,
    setApplied,
    setChosenUser,
    userLookup,
    resource,
    plans,
    selectedUser,
    editing,
    setEditing,
    editorLookup,
    editorUser,
    editorPlans,
    actions,
    focus,
    id,
    pagination,
  } = useSubscriptions({ initialUserId });
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
          disabled={!resource.ready || !plans.ready}
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
                          href={`/subscriptions/detail/?id=${encodeURIComponent(s.virtual_account_id)}`}
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
      <SubscriptionEditorDialog
        editing={editing}
        actions={actions}
        setEditing={setEditing}
        focus={focus}
        editorUser={editorUser}
        plans={plans}
        resource={resource}
        id={id}
        editorLookup={editorLookup}
        editorPlans={editorPlans}
      />
    </>
  );
}
