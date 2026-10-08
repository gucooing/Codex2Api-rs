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

export function ChatgptPluginRecords({ value }: { value: Json | undefined }) {
  const plugins = atPath(value, ["plugins"]);
  const rows = Array.isArray(plugins) ? plugins : [];
  return (
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
                  {scalar(atPath(plugin, ["release", "display_name"]) ?? atPath(plugin, ["name"]))}
                </div>
                <div className="text-xs text-muted-foreground">
                  {scalar(atPath(plugin, ["id"]))}
                </div>
              </TableCell>
              <TableCell>{typeof scope === "string" ? (scopes[scope] ?? scope) : "未知"}</TableCell>
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
              {value === undefined ? "—" : "暂无插件"}
            </TableCell>
          </TableRow>
        )}
      </TableBody>
    </Table>
  );
}
