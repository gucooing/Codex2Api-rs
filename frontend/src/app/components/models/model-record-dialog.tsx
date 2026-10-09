"use client";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardDescription } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldTitle } from "@/components/ui/field";
import { type Model } from "@/lib/api";
import { money } from "@/lib/format";
import { priceTierLabels } from "@/lib/model-pricing";
import { ChevronRight } from "lucide-react";

type Props = {
  model: Model;
};

export function ModelRecordDialog({ model }: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className="h-auto w-full min-w-0 justify-start gap-1 px-0 py-1 text-left md:hidden"
          aria-label={"查看详情：" + String(model.model)}
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">{model.model}</span>
            <span className="block truncate text-xs text-muted-foreground">
              {model.kind === "text" ? "文本 · Token" : "图像 · 按张"}
            </span>
          </span>
          <ChevronRight className="size-3 shrink-0" />
        </Button>
      </DialogTrigger>
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>记录详情</DialogTitle>
          <DialogDescription>当前记录的完整字段</DialogDescription>
        </DialogHeader>
        <FieldGroup className="gap-3">
          <Field>
            <FieldTitle>模型</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <strong>{model.model}</strong>
              <CardDescription>{model.provider_id}</CardDescription>
            </div>
          </Field>
          <Field>
            <FieldTitle>计费方式</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {model.kind === "text" ? "文本 · Token" : "图像 · 按张"}
            </div>
          </Field>
          <Field>
            <FieldTitle>价格规则</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              {model.kind === "text" ? (
                <>
                  <span>
                    {model.token_prices.length
                      ? `${model.token_prices.length} 条价格规则`
                      : "待定价"}
                  </span>
                  <CardDescription>
                    {[
                      ...new Set(model.token_prices.map((rule) => priceTierLabels[rule.tier])),
                    ].join(" · ") || "尚未配置价格"}
                  </CardDescription>
                  <CardDescription>美元 / 百万 Token</CardDescription>
                </>
              ) : (
                model.image_prices.map((rule) => (
                  <div key={rule.resolution}>
                    {rule.resolution}：{money(rule.price)} / 张
                  </div>
                ))
              )}
            </div>
          </Field>
          <Field>
            <FieldTitle>状态</FieldTitle>
            <div className="min-w-0 break-words [&_*]:max-w-full">
              <Badge variant={model.enabled ? "secondary" : "outline"}>
                {model.enabled ? "已启用" : "已停用"}
              </Badge>
            </div>
          </Field>
        </FieldGroup>
      </DialogContent>
    </Dialog>
  );
}
