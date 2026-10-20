import { ModelPlatformIcon } from "@/components/model-platforms";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { apiPrice, type ApiOperationPrice } from "@/lib/api-access";

export function SearchBillingCard({
  price,
  grouped,
}: {
  price: ApiOperationPrice | undefined;
  grouped: boolean;
}) {
  return (
    <Card size="sm" className="h-full min-w-0">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <ModelPlatformIcon platform="openai" className="size-5 shrink-0" />
          搜索
        </CardTitle>
        <CardDescription className="text-xs">OpenAI · 其他计费</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-xs text-muted-foreground">
          {grouped ? "计费价格" : "基础价格"} · USD / 次
        </p>
        <dl className="space-y-1">
          <dt className="text-xs text-muted-foreground">每次搜索</dt>
          <dd className="break-all font-medium tabular-nums">
            {price ? apiPrice(price.price_usd) : "—"}
          </dd>
        </dl>
      </CardContent>
    </Card>
  );
}
