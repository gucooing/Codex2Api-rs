"use client";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { imageResolutionTiers, type ImagePricingDraft } from "@/lib/model-pricing";
import { Plus, Trash2 } from "lucide-react";

export function ImagePricingFields({
  id,
  value,
  onChange,
}: {
  id: string;
  value: ImagePricingDraft;
  onChange: (value: ImagePricingDraft) => void;
}) {
  return (
    <FieldSet className="gap-3">
      <FieldLegend>图像价格（USD / 张）</FieldLegend>
      <Field className="max-w-64">
        <FieldLabel htmlFor={`${id}-image-mode`}>定价方式</FieldLabel>
        <Select
          value={value.mode}
          onValueChange={(mode: ImagePricingDraft["mode"]) => onChange({ ...value, mode })}
        >
          <SelectTrigger id={`${id}-image-mode`} className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            <SelectItem value="uniform">统一价格</SelectItem>
            <SelectItem value="resolution">按分辨率定价</SelectItem>
          </SelectContent>
        </Select>
      </Field>
      {value.mode === "uniform" ? (
        <Field className="max-w-64">
          <FieldLabel htmlFor={`${id}-image-uniform`}>每张价格 · 全部分辨率</FieldLabel>
          <Input
            id={`${id}-image-uniform`}
            type="number"
            min="0"
            step="0.000000001"
            required
            value={value.uniformPrice}
            onChange={(event) => onChange({ ...value, uniformPrice: event.target.value })}
          />
        </Field>
      ) : (
        <div className="space-y-3">
          {value.prices.map((row, index) => (
            <div className="flex flex-wrap items-end gap-2" key={index}>
              <Field className="w-48">
                <FieldLabel htmlFor={`${id}-image-${index}-resolution`}>分辨率档位</FieldLabel>
                <Select
                  value={row.resolution}
                  onValueChange={(resolution) =>
                    onChange({
                      ...value,
                      prices: value.prices.map((price, i) =>
                        i === index ? { ...price, resolution } : price,
                      ),
                    })
                  }
                >
                  <SelectTrigger id={`${id}-image-${index}-resolution`} className="w-full">
                    <SelectValue placeholder="请选择" />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    {imageResolutionTiers.map((tier) => (
                      <SelectItem
                        key={tier.value}
                        value={tier.value}
                        disabled={value.prices.some(
                          (price, i) => i !== index && price.resolution === tier.value,
                        )}
                      >
                        {tier.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              <Field className="w-40">
                <FieldLabel htmlFor={`${id}-image-${index}-price`}>每张价格</FieldLabel>
                <Input
                  id={`${id}-image-${index}-price`}
                  type="number"
                  min="0"
                  step="0.000000001"
                  required
                  value={row.price}
                  onChange={(event) =>
                    onChange({
                      ...value,
                      prices: value.prices.map((price, i) =>
                        i === index ? { ...price, price: event.target.value } : price,
                      ),
                    })
                  }
                />
              </Field>
              <Button
                type="button"
                variant="outline"
                size="icon"
                aria-label={`移除分辨率 ${index + 1}`}
                onClick={() =>
                  onChange({ ...value, prices: value.prices.filter((_, i) => i !== index) })
                }
              >
                <Trash2 />
              </Button>
            </div>
          ))}
          <Button
            type="button"
            variant="outline"
            disabled={value.prices.length >= imageResolutionTiers.length}
            onClick={() =>
              onChange({ ...value, prices: [...value.prices, { resolution: "", price: "" }] })
            }
          >
            <Plus />
            添加分辨率档位
          </Button>
        </div>
      )}
    </FieldSet>
  );
}
