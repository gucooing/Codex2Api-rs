"use client";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useRateLimitForm } from "@/lib/platform-account";

export function RateLimitForm({ id: accountId }: { id: string }) {
  const { id, actions, rpm, value, setDraft, handleSubmit } = useRateLimitForm({ id: accountId });
  return (
    <form noValidate className="space-y-3" onSubmit={handleSubmit}>
      <FieldSet disabled={!rpm.ready || actions.isBusy("consumer-rpm")}>
        <Field>
          <FieldLabel htmlFor={`${id}-rpm`}>RPM 限制</FieldLabel>
          <Input
            id={`${id}-rpm`}
            type="number"
            min={0}
            step={1}
            value={value}
            placeholder="继承默认限制"
            onChange={(event) => setDraft(event.target.value)}
          />
          <FieldDescription>
            留空用默认值（{rpm.data?.default_rpm ?? "—"}），0 为无限。
          </FieldDescription>
        </Field>
        <Button type="submit">保存 RPM</Button>
      </FieldSet>
    </form>
  );
}
