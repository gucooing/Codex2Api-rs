"use client";
import { ModelCallLimits } from "./model-call-limits";
import type { ModelMarketItem } from "@/app/data/model-market/pricing";
import { modelVendor } from "@/components/providers/model-vendor";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { apiPrice, apiTier } from "@/lib/api-access";

export function ModelMarketCard({
  item,
  grouped,
  onSelect,
}: {
  item: ModelMarketItem;
  grouped: boolean;
  onSelect: () => void;
}) {
  const { model, pricing } = item;
  const { Icon, name } = modelVendor(model.platform);
  const price = pricing.startingToken;
  return (
    <Card size="sm" className="relative h-full min-w-0">
      <CardHeader>
        <CardTitle className="flex min-w-0 items-center gap-2">
          <Icon className="size-5 shrink-0" />
          <Button
            type="button"
            variant="link"
            className="h-auto min-w-0 flex-1 shrink justify-start p-0 text-foreground after:absolute after:inset-0"
            aria-label={`查看 ${model.model} 的价格详情`}
            aria-haspopup="dialog"
            onClick={onSelect}
          >
            <span className="min-w-0 truncate" title={model.model}>
              {model.model}
            </span>
          </Button>
        </CardTitle>
        <CardDescription className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
          <span>
            {name} · {model.kind === "image" ? "图像" : "文本"}
          </span>
          {grouped && <span className="tabular-nums">倍率 {model.multiplier}×</span>}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-xs text-muted-foreground">
          {grouped ? "计费价格" : "基础价格"} ·{" "}
          {model.kind === "image" ? "USD / 张" : "USD / 百万 Token"}
          {model.kind === "text" &&
            price &&
            price.tier !== "standard" &&
            ` · ${apiTier(price.tier)}`}
        </p>
        {model.kind === "image" ? (
          <dl className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <dt className="text-xs text-muted-foreground">图像</dt>
              <dd className="break-all font-medium tabular-nums">
                {apiPrice(pricing.startingImage?.price)}
                {pricing.images.length > 1 && <span className="ml-1 text-xs font-normal">起</span>}
              </dd>
            </div>
            <div className="space-y-1">
              <dt className="text-xs text-muted-foreground">分辨率</dt>
              <dd className="break-all">
                {pricing.startingImage?.resolution === "*"
                  ? "全部分辨率"
                  : (pricing.startingImage?.resolution ?? "—")}
              </dd>
            </div>
          </dl>
        ) : (
          <dl className="grid grid-cols-2 gap-x-4 gap-y-3">
            <div className="space-y-1">
              <dt className="text-xs text-muted-foreground">输入</dt>
              <dd className="break-all font-medium tabular-nums">{apiPrice(price?.input)}</dd>
            </div>
            <div className="space-y-1">
              <dt className="text-xs text-muted-foreground">输出</dt>
              <dd className="break-all font-medium tabular-nums">{apiPrice(price?.output)}</dd>
            </div>
            <div className="space-y-1">
              <dt className="text-xs text-muted-foreground">缓存读取</dt>
              <dd className="break-all font-medium tabular-nums">{apiPrice(price?.cached)}</dd>
            </div>
            <div className="space-y-1">
              <dt className="text-xs text-muted-foreground">缓存写入</dt>
              <dd className="break-all font-medium tabular-nums">{apiPrice(price?.cacheWrite)}</dd>
            </div>
          </dl>
        )}
      </CardContent>
      {grouped && (
        <CardFooter>
          <ModelCallLimits model={model} />
        </CardFooter>
      )}
    </Card>
  );
}
