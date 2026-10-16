import type { ModelMarketItem } from "@/app/data/model-market/pricing";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { apiPrice, apiRange, apiTier } from "@/lib/api-access";

export function ModelPriceTable({ item }: { item: ModelMarketItem | undefined }) {
  const explicit = [
    { tier: "standard", prices: item?.pricing.standard ?? [] },
    ...(item?.pricing.services.filter((service) => service.multiplier == null) ?? []),
  ].filter((service) => service.prices.length);
  return item?.model.kind === "image" ? (
    <Table aria-label="图像价格">
      <TableHeader>
        <TableRow>
          <TableHead>分辨率</TableHead>
          <TableHead className="text-right">价格（USD / 张）</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {item.pricing.images.map((price) => (
          <TableRow key={price.resolution}>
            <TableCell>{price.resolution === "*" ? "全部分辨率" : price.resolution}</TableCell>
            <TableCell className="text-right tabular-nums">{apiPrice(price.price)}</TableCell>
          </TableRow>
        ))}
        {item.pricing.images.length === 0 && (
          <TableRow>
            <TableCell colSpan={2} className="text-center text-muted-foreground">
              待定价
            </TableCell>
          </TableRow>
        )}
      </TableBody>
    </Table>
  ) : (
    <div className="space-y-4">
      {explicit.map((service) => (
        <section
          key={service.tier}
          className="space-y-2"
          aria-label={`${apiTier(service.tier)}价格`}
        >
          <h3 className="text-sm font-medium">{apiTier(service.tier)}价格</h3>
          <Table aria-label={`${apiTier(service.tier)}阶梯价格`}>
            <TableHeader>
              <TableRow>
                <TableHead>输入区间（Token）</TableHead>
                <TableHead className="text-right">输入</TableHead>
                <TableHead className="text-right">输出</TableHead>
                <TableHead className="text-right">缓存读取</TableHead>
                <TableHead className="text-right">缓存写入</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {service.prices.map((price) => (
                <TableRow key={price.min_input_tokens}>
                  <TableCell>{apiRange(price)}</TableCell>
                  <TableCell className="text-right tabular-nums">{apiPrice(price.input)}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {apiPrice(price.output)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {apiPrice(price.cached)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {apiPrice(price.cacheWrite)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </section>
      ))}
      {item?.pricing.services.some((service) => service.multiplier != null) && (
        <section className="space-y-2" aria-label="服务档位倍率">
          <h3 className="text-sm font-medium">服务档位倍率</h3>
          <Table aria-label="服务档位倍率">
            <TableHeader>
              <TableRow>
                <TableHead>档位</TableHead>
                <TableHead>计费</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {item.pricing.services
                .filter((service) => service.multiplier != null)
                .map((service) => (
                  <TableRow key={service.tier}>
                    <TableCell>{apiTier(service.tier)}</TableCell>
                    <TableCell>标准价格 × {service.multiplier}</TableCell>
                  </TableRow>
                ))}
            </TableBody>
          </Table>
        </section>
      )}
      {item && !explicit.length && !item.pricing.services.length && (
        <p className="py-4 text-center text-muted-foreground">待定价</p>
      )}
    </div>
  );
}
