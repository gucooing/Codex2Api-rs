"use client";
import { useSupplierScheduling } from "@/app/data/suppliers";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";
import { useId } from "react";
const fields = [
  { key: "concurrency", label: "并发限制", min: 0, hint: "0 为无限制" },
  { key: "priority", label: "优先级", min: 0, hint: "数值越小越优先" },
  { key: "load_factor", label: "负载容量", min: 1, hint: "留空使用并发限制" },
] as const;
export function SupplierScheduling({ id }: { id: string }) {
  const fieldId = useId();
  const { resource, actions, value, update, submit } = useSupplierScheduling(id);
  const busy = actions.isBusy("supplier-scheduling");
  return (
    <form noValidate onSubmit={submit} className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="mr-auto text-sm text-muted-foreground">
          进行中：{resource.data?.runtime.active ?? "—"}　等待中：
          {resource.data?.runtime.waiting ?? "—"}
        </span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={resource.refreshing || busy}
          onClick={resource.reload}
        >
          刷新
        </Button>
        <Button type="submit" size="sm" disabled={!resource.ready || busy}>
          {busy && <Spinner />}保存
        </Button>
      </div>
      <FieldSet disabled={!resource.ready || busy}>
        <FieldGroup className="grid gap-4 sm:grid-cols-3">
          {fields.map((field) => (
            <Field key={field.key}>
              <FieldLabel htmlFor={`${fieldId}-${field.key}`}>{field.label}</FieldLabel>
              <Input
                id={`${fieldId}-${field.key}`}
                type="number"
                min={field.min}
                max={1000000}
                step={1}
                value={value(field.key)}
                onChange={(event) => update(field.key, event.target.value)}
              />
              <FieldDescription>{field.hint}</FieldDescription>
            </Field>
          ))}
        </FieldGroup>
      </FieldSet>
    </form>
  );
}
