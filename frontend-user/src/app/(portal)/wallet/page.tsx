"use client";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
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
import { useListResource } from "@/lib/pagination";
import { Badge } from "@/components/ui/badge";
import { date, money, type Entry } from "@/lib/api";
import { useErrorToast } from "@/lib/actions";
export default function WalletPage() {
  const wallet = useListResource<Entry, { balance_usd: string }>("/wallet");
  const walletPage = wallet.pagination;
  useErrorToast(wallet.error);
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" onClick={wallet.reload}>
          刷新
        </Button>
        <Badge variant="secondary">
          余额 {wallet.data ? money(wallet.data.balance_usd) : "—"} USD
        </Badge>
      </div>

      <Card>
        <CardContent className="space-y-3">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>时间</TableHead>
                <TableHead>操作</TableHead>
                <TableHead>变动前</TableHead>
                <TableHead>变动（USD）</TableHead>
                <TableHead>结余</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {wallet.data && wallet.data.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={5}>暂无钱包记录</TableCell>
                </TableRow>
              )}
              {walletPage.rows.map((e) => (
                <TableRow key={e.id}>
                  <TableCell>{date(e.created_at)}</TableCell>
                  <TableCell>{e.kind === "system_adjustment" ? "系统操作" : e.plan_name}</TableCell>
                  <TableCell>{money(e.balance_before_cents / 100)}</TableCell>
                  <TableCell>
                    {e.amount_cents > 0 ? "+" : ""}
                    {money(e.amount_cents / 100)}
                  </TableCell>
                  <TableCell>{money(e.balance_cents / 100)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end">
            <PaginationContent className="flex-wrap">
              <PaginationItem>
                共 {walletPage.total ?? "—"} 条 · {walletPage.pages ?? "—"} 页
              </PaginationItem>
              <PaginationItem>
                <Select {...walletPage.size}>
                  <SelectTrigger aria-label="每页条数">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent side="bottom">
                    {[10, 20, 30, 50].map((n) => (
                      <SelectItem key={n} value={String(n)}>
                        {n} 条
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...walletPage.first}>
                  首页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...walletPage.previous}>
                  上一页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Input className="w-16" aria-label="页码" {...walletPage.input} />
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...walletPage.next}>
                  下一页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...walletPage.last}>
                  末页
                </Button>
              </PaginationItem>
            </PaginationContent>
          </Pagination>
        </CardContent>
      </Card>
    </>
  );
}
