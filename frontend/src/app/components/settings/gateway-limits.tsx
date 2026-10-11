"use client";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { CircleAlert } from "lucide-react";
import type { GatewayForm } from "@/app/data/settings";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";

export function GatewayLimits({ form }: { form: GatewayForm }) {
  return (
    <FieldSet>
      <FieldLegend variant="label">请求限制</FieldLegend>
      <FieldGroup className="grid gap-4 sm:grid-cols-2">
        <Field>
          <div className="flex items-center gap-1">
            <FieldLabel htmlFor={form.rpm.id}>默认 RPM 限制</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="默认 RPM 限制说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" collisionPadding={16} aria-label="默认 RPM 限制说明">
                每个账户在最近一分钟内最多能发起多少次请求。账户单独设置了限制时，优先用该账户的值；0
                表示不限次数。
              </PopoverContent>
            </Popover>
          </div>
          <Input {...form.rpm} type="number" min={0} max={1000000} step={1} />
          <FieldDescription>0 为无限制</FieldDescription>
        </Field>
        <Field>
          <div className="flex items-center gap-1">
            <FieldLabel htmlFor={form.concurrency.id}>默认并发限制</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="默认并发限制说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" collisionPadding={16} aria-label="默认并发限制说明">
                每个账户同时可处理的请求数。例如填 5，最多同时处理 5
                个请求，超出时会提示稍后重试。账户单独设置时以该账户为准；0 表示不限制。
              </PopoverContent>
            </Popover>
          </div>
          <Input {...form.concurrency} type="number" min={0} max={1000000} step={1} />
          <FieldDescription>0 为无限制</FieldDescription>
        </Field>
      </FieldGroup>
    </FieldSet>
  );
}
