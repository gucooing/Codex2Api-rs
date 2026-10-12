"use client";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  apiLimit,
  apiPlatform,
  apiPrice,
  apiRange,
  apiTier,
  type ApiCatalogModel,
} from "@/lib/metered-api";

export function ModelPriceDialog({
  model,
  grouped,
  onClose,
}: {
  model: ApiCatalogModel | undefined;
  grouped: boolean;
  onClose: () => void;
}) {
  return (
    <Dialog
      open={!!model}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="flex max-h-[90dvh] flex-col sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle className="break-all">{model?.model ?? "模型价格"}</DialogTitle>
          <DialogDescription>
            {model ? apiPlatform(model.provider_id) : "—"} · {model?.multiplier ?? "—"}× ·{" "}
            {model?.kind === "image" ? "USD / 张" : "USD / 百万 Token"}
          </DialogDescription>
        </DialogHeader>
        <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[65dvh]">
          <div className="space-y-4 pr-3">
            {model?.kind === "image" ? (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>分辨率</TableHead>
                    <TableHead>基础价格</TableHead>
                    <TableHead>计费倍率</TableHead>
                    <TableHead>计费价格</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {model.image_prices.map((price) => (
                    <TableRow key={price.resolution}>
                      <TableCell>{price.resolution}</TableCell>
                      <TableCell>{apiPrice(price.base_price_usd)}</TableCell>
                      <TableCell>{model.multiplier}×</TableCell>
                      <TableCell>{apiPrice(price.price_usd)}</TableCell>
                    </TableRow>
                  ))}
                  {model.image_prices.length === 0 && (
                    <TableRow>
                      <TableCell colSpan={4}>待定价</TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>计费规则</TableHead>
                    <TableHead>项目</TableHead>
                    <TableHead>基础价格</TableHead>
                    <TableHead>计费倍率</TableHead>
                    <TableHead>计费价格</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {model?.prices.flatMap((price, index) =>
                    [
                      {
                        label: "普通输入",
                        base: price.base_input_usd_per_million,
                        effective: price.input_usd_per_million,
                      },
                      {
                        label: "输出",
                        base: price.base_output_usd_per_million,
                        effective: price.output_usd_per_million,
                      },
                      {
                        label: "缓存读取",
                        base: price.base_cached_usd_per_million,
                        effective: price.cached_usd_per_million,
                      },
                      {
                        label: "缓存写入",
                        base: price.base_cache_write_usd_per_million,
                        effective: price.cache_write_usd_per_million,
                      },
                    ].map((row, rowIndex) => (
                      <TableRow key={index + "-" + rowIndex}>
                        {rowIndex === 0 && (
                          <TableCell rowSpan={4}>
                            {apiTier(price.tier)}
                            <p className="text-xs text-muted-foreground">
                              {apiRange(price)} 输入 Token
                            </p>
                          </TableCell>
                        )}
                        <TableCell>{row.label}</TableCell>
                        <TableCell>{apiPrice(row.base)}</TableCell>
                        <TableCell>{model.multiplier}×</TableCell>
                        <TableCell>{apiPrice(row.effective)}</TableCell>
                      </TableRow>
                    )),
                  )}
                  {model?.prices.length === 0 && (
                    <TableRow>
                      <TableCell colSpan={5}>待定价</TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
            )}
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>每用户限制</TableHead>
                  <TableHead>并发</TableHead>
                  <TableHead>RPM</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <TableRow>
                  <TableCell>API 组内所有模型</TableCell>
                  <TableCell>
                    {model && grouped ? apiLimit(model.group_concurrency_limit) : "按 API 组规则"}
                  </TableCell>
                  <TableCell>
                    {model && grouped ? apiLimit(model.group_rpm_limit) : "按 API 组规则"}
                  </TableCell>
                </TableRow>
                <TableRow>
                  <TableCell>当前模型</TableCell>
                  <TableCell>
                    {model && grouped ? apiLimit(model.concurrency_limit) : "按 API 组规则"}
                  </TableCell>
                  <TableCell>
                    {model && grouped ? apiLimit(model.rpm_limit) : "按 API 组规则"}
                  </TableCell>
                </TableRow>
              </TableBody>
            </Table>
            <p className="text-xs text-muted-foreground">
              组与模型限制同时生效，同一用户在此组的所有密钥共同计数。
            </p>
          </div>
        </ScrollArea>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            关闭
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
