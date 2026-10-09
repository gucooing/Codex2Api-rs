"use client";
import { useDevicesPage } from "@/app/data/devices";
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
export default function DevicesPage() {
  const { devices, devicePage, actions, handleClick } = useDevicesPage();
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" onClick={devices.reload}>
          刷新
        </Button>
      </div>

      <Card>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>平台</TableHead>
                <TableHead>客户端</TableHead>
                <TableHead>最近使用</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {devices.data && devices.data.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={4}>暂无登录设备</TableCell>
                </TableRow>
              )}
              {devicePage.rows.map((d) => (
                <TableRow key={d.id}>
                  <TableCell>
                    {d.provider_id === "chatgpt"
                      ? "ChatGPT"
                      : d.provider_id === "grok"
                        ? "Grok"
                        : d.provider_id}
                  </TableCell>
                  <TableCell className="max-w-xs truncate">{d.user_agent}</TableCell>
                  <TableCell>{d.last_used_at ? date(d.last_used_at) : "尚未使用"}</TableCell>
                  <TableCell>
                    <Button
                      variant="outline"
                      disabled={!devices.ready || actions.isBusy(d.id)}
                      onClick={() => handleClick(d)}
                    >
                      撤销登录
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pagination className="mt-3 justify-end">
            <PaginationContent className="flex-wrap">
              <PaginationItem>
                共 {devicePage.total ?? "—"} 条 · {devicePage.pages ?? "—"} 页
              </PaginationItem>
              <PaginationItem>
                <Select {...devicePage.size}>
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
                <Button variant="outline" {...devicePage.first}>
                  首页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...devicePage.previous}>
                  上一页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Input className="w-16" aria-label="页码" {...devicePage.input} />
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...devicePage.next}>
                  下一页
                </Button>
              </PaginationItem>
              <PaginationItem>
                <Button variant="outline" {...devicePage.last}>
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
