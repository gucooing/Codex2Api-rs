import type { ApiGroup } from "@/lib/api-access";
import { apiLimit } from "@/lib/api-access";

export function ModelGroupLimits({ group }: { group: ApiGroup | null | undefined }) {
  return (
    <dl aria-label="分组调用限制" className="flex flex-wrap gap-4 text-xs">
      <div className="flex gap-2">
        <dt className="text-muted-foreground">分组并发</dt>
        <dd className="tabular-nums">{group ? apiLimit(group.concurrency_limit) : "—"}</dd>
      </div>
      <div className="flex gap-2">
        <dt className="text-muted-foreground">分组 RPM</dt>
        <dd className="tabular-nums">{group ? apiLimit(group.rpm_limit) : "—"}</dd>
      </div>
    </dl>
  );
}
