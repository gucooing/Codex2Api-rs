"use client";
import { ApiGroupEditorDialog } from "@/app/components/api-groups/api-group-editor-dialog";
import { emptyGroup, useApiGroupsPage } from "@/app/data/api-groups";
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
import { apiLimit, apiPlatform } from "@/lib/metered-api";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  MoreHorizontal,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
} from "lucide-react";

export default function ApiGroupsPage() {
  const {
    id,
    actions,
    filters,
    setFilters,
    resource,
    pagination,
    columns,
    editing,
    setEditing,
    search,
    reset,
    remove,
  } = useApiGroupsPage();
  return (
    <>
      <form className="flex flex-wrap items-end gap-3" onSubmit={search}>
        <Field className="w-44">
          <FieldLabel htmlFor={id + "-search"}>API 组</FieldLabel>
          <Input
            id={id + "-search"}
            placeholder="名称或介绍"
            value={filters.search}
            onChange={(event) => setFilters({ ...filters, search: event.target.value })}
          />
        </Field>
        <Field className="w-36">
          <FieldLabel htmlFor={id + "-platform"}>平台</FieldLabel>
          <Select
            value={filters.provider_id || "all"}
            onValueChange={(provider) =>
              setFilters({ ...filters, provider_id: provider === "all" ? "" : provider })
            }
          >
            <SelectTrigger id={id + "-platform"} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="all">全部平台</SelectItem>
              <SelectItem value="chatgpt">ChatGPT</SelectItem>
              <SelectItem value="grok">Grok</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Button type="submit">
          <Search />
          查询
        </Button>
        <Button type="button" variant="secondary" onClick={reset}>
          <RotateCcw />
          重置
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button type="button" variant="outline">
              <Columns3 />
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
        <div className="ml-auto flex gap-2">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="刷新 API 组"
            disabled={resource.refreshing}
            onClick={resource.reload}
          >
            <RefreshCw />
          </Button>
          <Button type="button" disabled={!resource.ready} onClick={() => setEditing(emptyGroup())}>
            <Plus />
            添加 API 组
          </Button>
        </div>
      </form>
      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                {columns.labels.map((label) => (
                  <TableHead key={label} hidden={!columns.isVisible(label)}>
                    {label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {pagination.rows.map((group) => (
                <TableRow key={group.id}>
                  <TableCell hidden={!columns.isVisible("名称")}>
                    <Button
                      type="button"
                      variant="link"
                      className="h-auto max-w-60 justify-start truncate p-0"
                      onClick={() => setEditing(group)}
                      disabled={!resource.ready}
                    >
                      {group.name}
                    </Button>
                    {group.description && (
                      <p
                        className="max-w-60 truncate text-xs text-muted-foreground"
                        title={group.description}
                      >
                        {group.description}
                      </p>
                    )}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("平台")}>
                    {apiPlatform(group.provider_id)}
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("模型范围")}>
                    {group.allowed_models.length
                      ? group.allowed_models.length + " 个指定模型"
                      : "该平台的全部模型"}
                    <p className="text-xs text-muted-foreground">
                      {group.tag_ids.length} 个关联标签
                    </p>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("倍率")}>
                    {group.multiplier}×
                    <p className="text-xs text-muted-foreground">
                      {group.model_rules.filter((rule) => rule.multiplier !== null).length}{" "}
                      个模型覆盖
                    </p>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("每用户限制")}>
                    <p>并发 {apiLimit(group.concurrency_limit)}</p>
                    <p>RPM {apiLimit(group.rpm_limit)}</p>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("状态")}>
                    <Badge variant={group.enabled ? "secondary" : "outline"}>
                      {group.enabled ? "已启用" : "已停用"}
                    </Badge>
                  </TableCell>
                  <TableCell hidden={!columns.isVisible("操作")}>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          aria-label={"操作 " + group.name}
                        >
                          <MoreHorizontal />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem
                          disabled={!resource.ready}
                          onSelect={() => setEditing(group)}
                        >
                          编辑
                        </DropdownMenuItem>
                        <DropdownMenuItem
                          variant="destructive"
                          disabled={
                            !resource.ready || actions.isBusy("api-group-delete:" + group.id)
                          }
                          onSelect={() => remove(group)}
                        >
                          删除
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              ))}
              {resource.ready && !pagination.rows.length && (
                <TableRow>
                  <TableCell colSpan={columns.count} className="text-center text-muted-foreground">
                    暂无 API 组
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end" aria-label="API 组分页">
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
              <PaginationItem className="text-xs text-muted-foreground">
                共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" size="icon-sm" aria-label="首页" {...pagination.first}>
                  <ChevronsLeft />
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button
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
                <Button variant="outline" size="icon-sm" aria-label="下一页" {...pagination.next}>
                  <ChevronRight />
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" size="icon-sm" aria-label="末页" {...pagination.last}>
                  <ChevronsRight />
                </Button>
              </PaginationItem>
            </PaginationContent>
          </Pagination>
        </CardContent>
      </Card>
      {editing && (
        <ApiGroupEditorDialog
          key={editing.id || "new"}
          group={editing}
          ready={resource.ready}
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
