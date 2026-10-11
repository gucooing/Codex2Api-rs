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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";

export function GatewayClientRules({ form }: { form: GatewayForm }) {
  return (
    <FieldSet>
      <FieldLegend variant="label">客户端规则</FieldLegend>
      <FieldGroup className="grid gap-4 sm:grid-cols-2">
        <Field>
          <div className="flex items-center gap-1">
            <FieldLabel htmlFor={`${form.id}-ua-mode`}>名单模式</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="名单模式说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" collisionPadding={16} aria-label="名单模式说明">
                黑名单会拦截命中规则的客户端，其他客户端可以使用；白名单只允许命中规则的客户端使用。白名单没有填写规则时，所有客户端都会被拒绝。
              </PopoverContent>
            </Popover>
          </div>
          <Select {...form.uaMode}>
            <SelectTrigger id={`${form.id}-ua-mode`} className="w-full">
              <SelectValue placeholder="尚未加载" />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="blacklist">黑名单</SelectItem>
              <SelectItem value="whitelist">白名单</SelectItem>
            </SelectContent>
          </Select>
          <FieldDescription>{form.uaHint}</FieldDescription>
        </Field>
        <Field>
          <div className="flex items-center gap-1">
            <FieldLabel htmlFor={`${form.id}-rules`}>UA 规则</FieldLabel>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="size-5 text-muted-foreground"
                  aria-label="UA 规则说明"
                >
                  <CircleAlert className="size-3" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" collisionPadding={16} aria-label="UA 规则说明">
                UA 是客户端发请求时带上的身份说明。按其中的文字匹配，不区分大小写，一行一条规则。*
                代表任意内容，例如 *curl* 能匹配包含 curl 的客户端标识。
              </PopoverContent>
            </Popover>
          </div>
          <Textarea id={`${form.id}-rules`} rows={4} {...form.rules} />
          <FieldDescription>每行一条，* 匹配任意字符</FieldDescription>
        </Field>
      </FieldGroup>
    </FieldSet>
  );
}
