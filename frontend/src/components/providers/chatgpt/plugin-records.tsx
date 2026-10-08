import { useListResource } from "@/lib/pagination";
import { useErrorToast } from "@/lib/actions";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { Json } from "@/lib/api";
import { atPath } from "@/lib/domain";
import { scalar } from "@/lib/records";

const scopes: Record<string, string> = {
  GLOBAL: "全局",
  USER: "个人",
  WORKSPACE: "工作区",
};

export function ChatgptPluginRecords({ path }: { path: string }) {
  const resource = useListResource<Json>(path, { section: "$.plugins" });
  const pagination = resource.pagination;
  const rows = pagination.rows;
  useErrorToast(resource.error);
  return (
    <>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>插件</TableHead>
            <TableHead>范围</TableHead>
            <TableHead>启用状态</TableHead>
            <TableHead>版本</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((plugin, index) => {
            const enabled = atPath(plugin, ["enabled"]);
            const scope = atPath(plugin, ["scope"]);
            return (
              <TableRow key={String(atPath(plugin, ["id"]) ?? index)}>
                <TableCell>
                  <div>
                    {scalar(
                      atPath(plugin, ["release", "display_name"]) ?? atPath(plugin, ["name"]),
                    )}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {scalar(atPath(plugin, ["id"]))}
                  </div>
                </TableCell>
                <TableCell>
                  {typeof scope === "string" ? (scopes[scope] ?? scope) : "未知"}
                </TableCell>
                <TableCell>
                  <Badge variant="outline">
                    {enabled === true ? "启用" : enabled === false ? "停用" : "未知"}
                  </Badge>
                </TableCell>
                <TableCell>{scalar(atPath(plugin, ["release", "version"]))}</TableCell>
              </TableRow>
            );
          })}
          {rows.length === 0 && (
            <TableRow>
              <TableCell colSpan={4} className="text-center text-muted-foreground">
                {!resource.data ? "—" : "暂无插件"}
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
      <Pagination>
        <PaginationContent>
          <PaginationItem>
            共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
          </PaginationItem>
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
    </>
  );
}
