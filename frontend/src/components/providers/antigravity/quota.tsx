import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import type { SupplierQuotaGroup } from "@/lib/api";
import { antigravityQuotaGroups } from "@/lib/providers/antigravity/data";
import { percentLabel, quotaResetLabel } from "@/lib/supplier-state";

export function AntigravityQuotaGroups({
  groups = [],
  now,
}: {
  groups?: SupplierQuotaGroup[];
  now: number;
}) {
  return (
    <div className="w-full max-w-md space-y-3">
      {antigravityQuotaGroups(groups).map((group, index) => (
        <div key={`${group.name}-${index}`} className="space-y-2">
          {index > 0 && <Separator />}
          <div className="text-xs text-muted-foreground">{group.name}</div>
          {group.windows.map((window, windowIndex) => (
            <div key={window.id || windowIndex} className="space-y-1">
              <div className="flex flex-wrap items-center justify-between gap-x-2 text-xs">
                <span>{window.name}</span>
                <span className="text-muted-foreground">
                  {window.disabled ? "不可用" : quotaResetLabel(window.resetAt, now)}
                </span>
              </div>
              <div className="flex items-center gap-2">
                <Progress
                  value={window.remainingPercent ?? undefined}
                  aria-label={`${group.name} ${window.name}`}
                />
                <span className="text-xs tabular-nums">
                  {window.remainingPercent == null ? "未知" : percentLabel(window.remainingPercent)}
                </span>
              </div>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}
