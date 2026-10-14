"use client";
import { useSubscriptionsPage } from "@/app/data/subscriptions";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
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
import { date } from "@/lib/api";
import Link from "next/link";
export default function SubscriptionsPage() {
  const { subscriptions, subscriptionPage } = useSubscriptionsPage();
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" onClick={subscriptions.reload}>
          刷新
        </Button>
        <Button asChild>
          <Link href="/plans/">购买套餐</Link>
        </Button>
      </div>

      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>平台</TableHead>
                <TableHead>套餐</TableHead>
                <TableHead>到期时间</TableHead>
                <TableHead>RPM 限制</TableHead>
                <TableHead>并发限制</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {subscriptionPage.rows.map((s) => (
                <TableRow key={s.id}>
                  <TableCell>
                    {s.provider_id === "chatgpt"
                      ? "ChatGPT"
                      : s.provider_id === "grok"
                        ? "Grok"
                        : s.provider_id}
                  </TableCell>
                  <TableCell>
                    {s.plan_name}
                    <div className="text-xs text-muted-foreground">{s.id.slice(0, 8)}</div>
                  </TableCell>
                  <TableCell>{date(s.expires_at)}</TableCell>
                  <TableCell>{s.rpm_limit === 0 ? "不限" : s.rpm_limit}</TableCell>
                  <TableCell>{s.concurrency_limit === 0 ? "不限" : s.concurrency_limit}</TableCell>
                  <TableCell>
                    <Badge variant="secondary">
                      {!s.enabled ? "已停用" : s.expired ? "已到期" : "有效"}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <Button asChild variant="outline" size="sm">
                      <Link href={`/plans/?subscription_id=${encodeURIComponent(s.id)}`}>
                        续订 / 升级
                      </Link>
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {subscriptions.ready && !subscriptions.data?.items.length && (
                <TableRow>
                  <TableCell colSpan={7}>暂无订阅，可在“购买套餐”中选择。</TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end">
            <PaginationContent className="flex-wrap">
              <PaginationItem>
                共 {subscriptionPage.total ?? "—"} 条 · {subscriptionPage.pages ?? "—"} 页
              </PaginationItem>
              <PaginationItem>
                <Select {...subscriptionPage.size}>
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
                <Button variant="outline" {...subscriptionPage.first}>
                  首页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...subscriptionPage.previous}>
                  上一页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Input className="w-16" aria-label="页码" {...subscriptionPage.input} />
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...subscriptionPage.next}>
                  下一页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...subscriptionPage.last}>
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
