"use client";
import { Fragment } from "react";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { SupplierQuotaGroup } from "@/lib/api";
import { antigravityQuotaRows } from "@/lib/providers/antigravity/data";
import { AntigravityQuotaGroups } from "./quota";

export function AntigravityQuotaSummary({
  groups = [],
  now,
}: {
  groups?: SupplierQuotaGroup[];
  now: number;
}) {
  return (
    <div className="w-full space-y-2 text-xs" role="group" aria-label="Antigravity 模型额度">
      {antigravityQuotaRows(groups).map((row) => (
        <div key={row.id} className="flex items-center gap-2" role="group" aria-label={row.label}>
          <div className="flex min-w-0 flex-1 items-center">
            {row.segments.map((segment, index) => (
              <Fragment key={`${segment.group.name}-${segment.id}-${index}`}>
                {index > 0 && (
                  <Separator orientation="vertical" className="h-2 data-vertical:self-center" />
                )}
                <Popover>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <PopoverTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="xs"
                          className="h-4 min-w-0 flex-1 rounded-none border-0 p-0 hover:bg-transparent aria-expanded:bg-transparent dark:hover:bg-transparent"
                          title=""
                          aria-label={`${segment.group.name} ${row.label}，查看额度详情`}
                        >
                          <Progress
                            value={segment.remainingPercent ?? undefined}
                            className={cn(
                              "h-1 flex-1 rounded-none [&>[data-slot=progress-indicator]]:bg-foreground",
                              index === 0 && "rounded-l-full",
                              index === row.segments.length - 1 && "rounded-r-full",
                            )}
                            aria-label={`${segment.group.name} ${row.label} 剩余额度`}
                            aria-valuenow={segment.remainingPercent ?? undefined}
                            aria-valuetext={
                              segment.remainingPercent == null
                                ? "未知"
                                : `${segment.remainingPercent}%`
                            }
                          />
                        </Button>
                      </PopoverTrigger>
                    </TooltipTrigger>
                    <TooltipContent side="top">{segment.group.name}</TooltipContent>
                  </Tooltip>
                  <PopoverContent
                    align="start"
                    className="max-h-96 w-80 max-w-[calc(100vw-2rem)] overflow-y-auto"
                    aria-label={`${segment.group.name} ${row.label} 额度详情`}
                  >
                    <AntigravityQuotaGroups groups={[segment.group]} now={now} />
                  </PopoverContent>
                </Popover>
              </Fragment>
            ))}
          </div>
          <span className="w-9 shrink-0 whitespace-nowrap text-right tabular-nums">
            {row.label}
          </span>
        </div>
      ))}
    </div>
  );
}
