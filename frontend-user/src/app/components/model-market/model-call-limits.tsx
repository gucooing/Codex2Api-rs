import type { ApiCatalogModel } from "@/lib/api-access";
import { apiLimit } from "@/lib/api-access";

export function ModelCallLimits({ model }: { model: ApiCatalogModel }) {
  return (
    <dl aria-label="模型调用限制" className="flex flex-wrap gap-4 text-xs">
      <div className="flex gap-2">
        <dt className="text-muted-foreground">并发</dt>
        <dd className="tabular-nums">{apiLimit(model.concurrency_limit)}</dd>
      </div>
      <div className="flex gap-2">
        <dt className="text-muted-foreground">RPM</dt>
        <dd className="tabular-nums">{apiLimit(model.rpm_limit)}</dd>
      </div>
    </dl>
  );
}
