"use client";
import { RateLimitForm } from "@/components/forms/platform-rate-limit-form";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel, FieldSet } from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { type Consumer } from "@/lib/api";
import { type RoutingResponse, useRoutingForm } from "@/lib/platform-account";
export function RoutingForm({
  id: accountId,
  account,
  data,
  disabled,
  onSaved,
}: {
  id: string;
  account?: Consumer;
  data: RoutingResponse;
  disabled: boolean;
  onSaved: () => void;
}) {
  const { id, actions, selectTag, current, tags, handleSubmit } = useRoutingForm({
    account,
    data,
    disabled,
    onSaved,
  });
  return (
    <div className="space-y-4">
      <form noValidate className="space-y-3" onSubmit={handleSubmit}>
        <FieldSet disabled={disabled || !tags.ready || actions.isBusy("pool-route")}>
          <Field>
            <FieldLabel htmlFor={`${id}-tag`}>标签号池</FieldLabel>
            <Select value={current.tag || "none"} onValueChange={selectTag}>
              <SelectTrigger id={`${id}-tag`} className="w-full">
                <SelectValue placeholder="选择标签号池" />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="none">未配置</SelectItem>
                {(tags.data?.items ?? []).map((tag) => (
                  <SelectItem key={tag.id} value={tag.id}>
                    {tag.name}（{tag.supplier_count} 个供应账户）
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Button type="submit">保存号池</Button>
        </FieldSet>
      </form>
      <RateLimitForm id={accountId} />
    </div>
  );
}
