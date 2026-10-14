"use client";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useRateLimitForm } from "@/lib/platform-account";

export function RateLimitForm({ id: accountId }: { id: string }) {
  const { id, actions, rpm, value, setDraft, concurrency, setConcurrencyDraft, handleSubmit } =
    useRateLimitForm({ id: accountId });
  return (
    <form noValidate className="space-y-3" onSubmit={handleSubmit}>
      <FieldSet disabled={!rpm.ready || actions.isBusy("consumer-rpm")}>
        <FieldGroup className="grid gap-4 sm:grid-cols-2">
          <Field>
            <FieldLabel htmlFor={`${id}-rpm`}>RPM 限制</FieldLabel>
            <Input
              id={`${id}-rpm`}
              type="number"
              min={0}
              max={1000000}
              step={1}
              value={value}
              placeholder="继承套餐限制"
              onChange={(event) => setDraft(event.target.value)}
            />
            <FieldDescription>
              留空继承套餐（{rpm.data?.plan_rpm ?? "—"}），0 为无限。
            </FieldDescription>
          </Field>
          <Field>
            <FieldLabel htmlFor={`${id}-concurrency`}>并发限制</FieldLabel>
            <Input
              id={`${id}-concurrency`}
              type="number"
              min={0}
              max={1000000}
              step={1}
              value={concurrency}
              placeholder="继承套餐限制"
              onChange={(event) => setConcurrencyDraft(event.target.value)}
            />
            <FieldDescription>
              留空继承套餐（{rpm.data?.plan_concurrency ?? "—"}），0 为无限。
            </FieldDescription>
          </Field>
        </FieldGroup>
        <Button type="submit" className="self-end">
          保存限制
        </Button>
      </FieldSet>
    </form>
  );
}
