"use client";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { CircleAlert } from "lucide-react";
import { useState } from "react";
import type { GatewayForm } from "@/app/data/settings";
import { Field, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

const schedulingModes = [
  {
    value: "advanced",
    label: "高级评分",
    description:
      "综合忙碌程度、排队人数、最近失败情况和响应速度，从表现较好的账户中选择，也会考虑设置的优先级。",
  },
  {
    value: "traditional",
    label: "传统负载",
    description:
      "先看优先级，再选较空闲的账户；条件相同时，优先使用更久没被使用的账户，分配规则更直接。",
  },
] as const;

export function GatewayScheduling({ form }: { form: GatewayForm }) {
  const [activeMode, setActiveMode] = useState<string | null>(null);

  return (
    <FieldSet>
      <FieldLegend variant="label">号池调度</FieldLegend>
      <FieldGroup className="grid gap-4 sm:grid-cols-2">
        <Field>
          <div className="flex items-center gap-1">
            <FieldLabel htmlFor={`${form.id}-mode`}>调度方式</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="调度方式说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" collisionPadding={16} aria-label="调度方式说明">
                {schedulingModes.map((mode) => (
                  <p key={mode.value}>
                    <strong>{mode.label}：</strong>
                    {mode.description}
                  </p>
                ))}
              </PopoverContent>
            </Popover>
          </div>
          <Select {...form.mode} onOpenChange={() => setActiveMode(null)}>
            <Tooltip delayDuration={300} disableHoverableContent>
              <TooltipTrigger asChild>
                <SelectTrigger id={`${form.id}-mode`} className="w-full">
                  <SelectValue placeholder="尚未加载" />
                </SelectTrigger>
              </TooltipTrigger>
              <TooltipContent>
                {schedulingModes.find((mode) => mode.value === form.mode.value)?.description ??
                  "选择如何为请求分配供应账户。"}
              </TooltipContent>
            </Tooltip>
            <SelectContent position="popper">
              {schedulingModes.map((mode) => (
                <Tooltip key={mode.value} open={activeMode === mode.value} disableHoverableContent>
                  <TooltipTrigger asChild>
                    <SelectItem
                      value={mode.value}
                      onFocus={() => setActiveMode(mode.value)}
                      onBlur={() =>
                        setActiveMode((active) => (active === mode.value ? null : active))
                      }
                      onPointerMove={(event) => {
                        if (event.pointerType !== "touch") setActiveMode(mode.value);
                      }}
                      onPointerLeave={() => setActiveMode(null)}
                    >
                      {mode.label}
                    </SelectItem>
                  </TooltipTrigger>
                  <TooltipContent side="right" sideOffset={8}>
                    {mode.description}
                  </TooltipContent>
                </Tooltip>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <div className="flex items-center gap-1">
            <FieldLabel htmlFor={form.candidates.id}>候选账户数</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="候选账户数说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" collisionPadding={16} aria-label="候选账户数说明">
                仅高级评分使用。每次从评分最高的前几个账户中挑选。填 3 表示只在排名前 3
                的账户中选择；数值越大，参与分配的账户越多。
              </PopoverContent>
            </Popover>
          </div>
          <Input {...form.candidates} type="number" min={1} max={100} step={1} />
        </Field>
        <Field>
          <div className="flex items-center gap-1">
            <FieldLabel htmlFor={form.sessionTtl.id}>会话有效期（秒）</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="会话有效期（秒）说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" collisionPadding={16} aria-label="会话有效期（秒）说明">
                同一段对话优先沿用同一个供应账户，这项设置决定保留多久。继续沿用会重新计时，超时后会重新选择。3600
                秒就是 1 小时。
              </PopoverContent>
            </Popover>
          </div>
          <Input {...form.sessionTtl} type="number" min={60} max={604800} step={1} />
        </Field>
        <Field>
          <div className="flex items-center gap-1">
            <FieldLabel htmlFor={form.wait.id}>粘连等待时间（毫秒）</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="粘连等待时间（毫秒）说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent
                align="start"
                collisionPadding={16}
                aria-label="粘连等待时间（毫秒）说明"
              >
                原来使用的供应账户已经忙满、又不能切换时，最多等多久。超过时间仍没有空位，会提示稍后重试。1000
                毫秒就是 1 秒。
              </PopoverContent>
            </Popover>
          </div>
          <Input {...form.wait} type="number" min={1} max={60000} step={1} />
        </Field>
        <Field orientation="horizontal">
          <div className="flex flex-1 items-center gap-1">
            <FieldLabel htmlFor={`${form.id}-weighted`}>粘性加权</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="粘性加权说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" collisionPadding={16} aria-label="粘性加权说明">
                仅高级评分使用。开启后，原来使用的账户会获得额外优先考虑，同时比较其他账户的状态；关闭后，会优先继续使用原账户。
              </PopoverContent>
            </Popover>
          </div>
          <Switch id={`${form.id}-weighted`} {...form.weighted} />
        </Field>
        <Field orientation="horizontal">
          <div className="flex flex-1 items-center gap-1">
            <FieldLabel htmlFor={`${form.id}-escape`}>允许粘连逃逸</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="允许粘连逃逸说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" collisionPadding={16} aria-label="允许粘连逃逸说明">
                优先沿用原账户时，开启后遇到该账户忙满、最近失败较多或响应较慢，可以改选其他账户。关闭后会等待原账户。当前对话无法换账户时，仍需等待。
              </PopoverContent>
            </Popover>
          </div>
          <Switch id={`${form.id}-escape`} {...form.escape} />
        </Field>
      </FieldGroup>
    </FieldSet>
  );
}
