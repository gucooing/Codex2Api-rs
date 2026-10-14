"use client";
import { useUserRecords } from "@/app/data/users";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { date, money } from "@/lib/format";
import Link from "next/link";

export function UserRecords({ id, close }: { id: string; close: () => void }) {
  const { resource, pagination } = useUserRecords({ id });
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
