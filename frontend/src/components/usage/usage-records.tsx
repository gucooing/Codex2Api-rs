"use client";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
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
import { UsageTable } from "@/components/usage/usage-table";
import { validateForm } from "@/lib/actions";
import {
  emptyFilters,
  useUsagePageView,
  type ConsumerOption,
  type SupplierOption,
} from "@/lib/usage";
import { usageStatuses } from "@/lib/usage-display";
import { userOptionLabel, type UserOption } from "@/lib/user-lookup";
import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  RotateCcw,
  Search,
} from "lucide-react";

export function UsagePageView({ consumerId }: { consumerId?: string }) {
  const {
    fieldId,
    userLookup,
    filters,
    setFilters,
    setApplied,
    setPage,
    supplierOpen,
    setSupplierOpen,
    consumerOpen,
    setConsumerOpen,
    setSupplierSearch,
    setConsumerSearch,
    selectedUser,
    selectedSupplier,
    selectedConsumer,
    suppliers,
    consumers,
    resource,
    pagination,
    update,
  } = useUsagePageView({ consumerId });
  return (
    <>
      <Card>
        <CardContent className="space-y-4">
          <form
            className="space-y-4"
            noValidate
            onSubmit={(e) => {
              e.preventDefault();
              if (!validateForm(e.currentTarget)) return;
              setApplied(filters);
              resource.reload();
              setPage(1);
            }}
          >
            <Collapsible>
              <div className="flex flex-wrap items-end gap-3">
                {!consumerId && (
                  <Field className="w-40">
                    <FieldLabel className="sr-only" htmlFor={fieldId + "-user"}>
                      用户
                    </FieldLabel>
                    <Combobox<UserOption>
                      items={userLookup.data?.items ?? []}
                      value={selectedUser}
                      onValueChange={(item) => {
                        setFilters((v) => ({
                          ...v,
                          user_id: item?.id ?? "",
                          user_label: item ? userOptionLabel(item) : "",
                        }));
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
                            setFilters((v) => ({ ...v, user_id: "", user_label: "" }));
                          }
                        }
                      }}
                    >
                      <ComboboxInput
                        id={fieldId + "-user"}
                        placeholder="搜索选择用户"
                        showClear
                        className="w-full"
                        maxLength={128}
                      />
                      <ComboboxContent>
                        <ComboboxEmpty>
                          {userLookup.loading
                            ? "加载中…"
                            : userLookup.error
                              ? "加载失败"
                              : "没有匹配用户"}
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
                )}
                <Field className="w-40">
                  <FieldLabel htmlFor={fieldId + "-supplier"}>供应账户</FieldLabel>
                  <Combobox<SupplierOption>
                    items={suppliers.data?.items ?? []}
                    value={selectedSupplier ?? null}
                    onValueChange={(item) => {
                      setFilters((current) => ({
                        ...current,
                        supplier_id: item?.id ?? "",
                        supplier_label: item?.display_name || item?.email || item?.id || "",
                      }));
                      setSupplierSearch("");
                    }}
                    itemToStringLabel={(item) => item.display_name || item.email || item.id}
                    itemToStringValue={(item) => item.id}
                    isItemEqualToValue={(item, value) => item.id === value.id}
                    filter={null}
                    open={supplierOpen}
                    onOpenChange={(open, details) => {
                      setSupplierOpen(open);
                      if (open && details.reason !== "input-change") setSupplierSearch("");
                    }}
                    onInputValueChange={(text, details) => {
                      if (details.reason === "input-change") {
                        setSupplierSearch(text);
                        if (!text) {
                          setFilters((current) => ({
                            ...current,
                            supplier_id: "",
                            supplier_label: "",
                          }));
                        }
                      }
                    }}
                  >
                    <ComboboxInput
                      id={fieldId + "-supplier"}
                      aria-label="供应账户"
                      placeholder="全部账户"
                      showClear
                      className="w-full"
                    />
                    <ComboboxContent>
                      <ComboboxEmpty>
                        {suppliers.loading
                          ? "正在加载…"
                          : suppliers.error
                            ? "加载失败，请重新搜索"
                            : "没有匹配账户"}
                      </ComboboxEmpty>
                      <ComboboxList aria-busy={suppliers.loading}>
                        {(item: SupplierOption) => (
                          <ComboboxItem key={item.id} value={item}>
                            <span className="flex min-w-0 flex-col">
                              <span className="truncate">
                                {item.display_name || item.email || item.id}
                              </span>
                              <span className="truncate text-xs text-muted-foreground">
                                {item.email}
                              </span>
                            </span>
                          </ComboboxItem>
                        )}
                      </ComboboxList>
                    </ComboboxContent>
                  </Combobox>
                </Field>
                {!consumerId && (
                  <Field className="w-40">
                    <FieldLabel htmlFor={fieldId + "-consumer"}>消费账户</FieldLabel>
                    <Combobox<ConsumerOption>
                      items={consumers.data?.items ?? []}
                      value={selectedConsumer ?? null}
                      onValueChange={(item) => {
                        setFilters((current) => ({
                          ...current,
                          virtual_account: item?.id ?? "",
                          consumer_label: item?.username ?? "",
                        }));
                        setConsumerSearch("");
                      }}
                      itemToStringLabel={(item) => item.username}
                      itemToStringValue={(item) => item.id}
                      isItemEqualToValue={(item, value) => item.id === value.id}
                      filter={null}
                      open={consumerOpen}
                      onOpenChange={(open, details) => {
                        setConsumerOpen(open);
                        if (open && details.reason !== "input-change") setConsumerSearch("");
                      }}
                      onInputValueChange={(text, details) => {
                        if (details.reason === "input-change") {
                          setConsumerSearch(text);
                          if (!text) {
                            setFilters((current) => ({
                              ...current,
                              virtual_account: "",
                              consumer_label: "",
                            }));
                          }
                        }
                      }}
                    >
                      <ComboboxInput
                        id={fieldId + "-consumer"}
                        aria-label="消费账户"
                        placeholder="全部账户"
                        showClear
                        className="w-full"
                      />
                      <ComboboxContent>
                        <ComboboxEmpty>
                          {consumers.loading
                            ? "正在加载…"
                            : consumers.error
                              ? "加载失败，请重新搜索"
                              : "没有匹配账户"}
                        </ComboboxEmpty>
                        <ComboboxList aria-busy={consumers.loading}>
                          {(item: ConsumerOption) => (
                            <ComboboxItem key={item.id} value={item}>
                              <span className="flex min-w-0 flex-col">
                                <span className="truncate">{item.username}</span>
                                <span className="truncate text-xs text-muted-foreground">
                                  {item.email}
                                </span>
                              </span>
                            </ComboboxItem>
                          )}
                        </ComboboxList>
                      </ComboboxContent>
                    </Combobox>
                  </Field>
                )}
                <Field className="w-40">
                  <FieldLabel
                    htmlFor={fieldId + "-field-3" + "-" + encodeURIComponent(String("模型"))}
                  >
                    {"模型"}
                  </FieldLabel>
                  <Input
                    id={fieldId + "-field-3" + "-" + encodeURIComponent(String("模型"))}
                    aria-label={"模型"}
                    value={filters.model}
                    onChange={(e) => update("model", e.target.value)}
                  />
                </Field>
                <Field className="w-40">
                  <FieldLabel
                    htmlFor={fieldId + "-field-4" + "-" + encodeURIComponent(String("状态"))}
                  >
                    {"状态"}
                  </FieldLabel>
                  <Select
                    value={filters.status}
                    onValueChange={(next) =>
                      ((value) => update("status", value))(
                        next ===
                          fieldId + "-field-4" + "-" + encodeURIComponent(String("状态")) + "-empty"
                          ? ""
                          : next,
                      )
                    }
                  >
                    <SelectTrigger
                      id={fieldId + "-field-4" + "-" + encodeURIComponent("状态")}
                      aria-label="状态"
                      className="w-full"
                    >
                      <SelectValue placeholder="全部状态" />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem
                        value={fieldId + "-field-4" + "-" + encodeURIComponent("状态") + "-empty"}
                      >
                        全部状态
                      </SelectItem>
                      {usageStatuses.map((status) => (
                        <SelectItem key={status.value} value={status.value}>
                          {status.label}
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
                    variant="outline"
                    type="button"
                    onClick={() => {
                      const next = { ...emptyFilters, virtual_account: consumerId ?? "" };
                      setSupplierSearch("");
                      setConsumerSearch("");
                      setFilters(next);
                      setApplied(next);
                      resource.reload();
                      setPage(1);
                    }}
                  >
                    <RotateCcw />
                    重置
                  </Button>
                  <CollapsibleTrigger asChild>
                    <Button type="button" variant="ghost">
                      <CalendarDays />
                      时间范围{filters.from || filters.until ? " · 已设置" : ""}
                    </Button>
                  </CollapsibleTrigger>
                </div>
              </div>
              <CollapsibleContent>
                <div className="mt-3 flex flex-wrap items-end gap-3">
                  <Field className="w-56">
                    <FieldLabel
                      htmlFor={fieldId + "-field-5" + "-" + encodeURIComponent(String("开始时间"))}
                    >
                      {"开始时间"}
                    </FieldLabel>
                    <Input
                      id={fieldId + "-field-5" + "-" + encodeURIComponent(String("开始时间"))}
                      aria-label={"开始时间"}
                      type="datetime-local"
                      value={filters.from}
                      max={filters.until || undefined}
                      onChange={(event) => update("from", event.target.value)}
                    />
                  </Field>
                  <Field className="w-56">
                    <FieldLabel
                      htmlFor={fieldId + "-field-6" + "-" + encodeURIComponent(String("结束时间"))}
                    >
                      {"结束时间"}
                    </FieldLabel>
                    <Input
                      id={fieldId + "-field-6" + "-" + encodeURIComponent(String("结束时间"))}
                      aria-label={"结束时间"}
                      type="datetime-local"
                      value={filters.until}
                      min={filters.from || undefined}
                      onChange={(event) => update("until", event.target.value)}
                    />
                  </Field>
                </div>
              </CollapsibleContent>
            </Collapsible>
          </form>
        </CardContent>
      </Card>

      {
        <>
          <Card>
            <CardContent className="space-y-4">
              <UsageTable records={resource.data?.records ?? []} />
              <Pagination aria-label="记录分页" className="justify-end">
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
        </>
      }
    </>
  );
}
