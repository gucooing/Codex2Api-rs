"use client";

import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { UsageRecord } from "@/lib/usage";
import { money } from "@/lib/format";
import { usageFormula } from "@/lib/usage-calculation";

export function UsageFee({
  record,
}: {
  record: Pick<UsageRecord, "cost_nano_usd" | "billing_calculation">;
}) {
  const amount = record.cost_nano_usd == null ? "-" : money(record.cost_nano_usd / 1e9);
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto px-0 py-0 text-xs tabular-nums"
          aria-label={`查看费用公式：${amount}`}
        >
          {amount}
        </Button>
      </PopoverTrigger>
      <PopoverContent
        className="w-auto max-w-[min(40rem,calc(100vw-2rem))] break-words text-xs"
        aria-label="费用计算公式"
      >
        {usageFormula(record.billing_calculation)}
      </PopoverContent>
    </Popover>
  );
}
