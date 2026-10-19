"use client";
import { RateLimitForm } from "@/components/forms/platform-rate-limit-form";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldDescription, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { type SubscriptionAccount } from "@/lib/api";
import { type RoutingResponse, useRoutingForm } from "@/lib/platform-account";
import { providerLabel } from "@/lib/providers";
export function RoutingForm({
  id: accountId,
  account,
  data,
  disabled,
  onSaved,
}: {
  id: string;
  account?: SubscriptionAccount;
  data: RoutingResponse;
  disabled: boolean;
  onSaved: () => void;
}) {
  const { id, actions, inherit, setInherit, selected, toggleTag, effective, tags, handleSubmit } =
    useRoutingForm({ account, data, disabled, onSaved });
  const tagName = (tagId: string) =>
    tags.data?.items.find((tag) => tag.id === tagId)?.name ?? tagId;
  return (
    <div className="space-y-4">
      <form noValidate className="space-y-3" onSubmit={handleSubmit}>
        <FieldSet disabled={disabled || !tags.ready || actions.isBusy("pool-route")}>
          <Field orientation="horizontal">
            <Switch id={`${id}-inherit`} checked={inherit} onCheckedChange={setInherit} />
            <FieldLabel htmlFor={`${id}-inherit`}>继承套餐号池</FieldLabel>
          </Field>
          {!inherit && (
            <FieldSet>
              <FieldLegend variant="label">订阅专属号池标签</FieldLegend>
              <div className="grid gap-2 sm:grid-cols-2">
                {(tags.data?.items ?? []).map((tag) => (
                  <Field key={tag.id} orientation="horizontal">
                    <Checkbox
                      id={`${id}-tag-${tag.id}`}
                      checked={selected.includes(tag.id)}
                      onCheckedChange={(checked) => toggleTag(tag.id, checked === true)}
                    />
                    <FieldLabel htmlFor={`${id}-tag-${tag.id}`}>
                      {tag.name} · {providerLabel(tag.provider_id)}（{tag.supplier_count}{" "}
                      个供应账户）
                    </FieldLabel>
                  </Field>
                ))}
              </div>
            </FieldSet>
          )}
          <FieldDescription>
            当前生效：{effective.length ? effective.map(tagName).join("、") : "无号池"}
          </FieldDescription>
          <Button type="submit">保存号池</Button>
        </FieldSet>
      </form>
      <RateLimitForm id={accountId} />
    </div>
  );
}
