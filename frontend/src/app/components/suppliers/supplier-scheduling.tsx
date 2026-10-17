"use client";
import { useSupplierScheduling } from "@/app/data/suppliers";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";
import { RefreshCw } from "lucide-react";
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
    <Card>
      <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-2">
        <div className="space-y-1">
          <CardTitle role="heading" aria-level={2}>
            调度配置
          </CardTitle>
          <CardDescription>
            进行中：{resource.data?.runtime.active ?? "—"}　等待中：
            {resource.data?.runtime.waiting ?? "—"}
          </CardDescription>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={resource.refreshing || busy}
            onClick={resource.reload}
          >
            <RefreshCw />
            刷新
          </Button>
          <Button
            type="submit"
            form={`${fieldId}-form`}
            size="sm"
            disabled={!resource.ready || busy}
          >
            {busy && <Spinner />}保存
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        <form id={`${fieldId}-form`} noValidate onSubmit={submit}>
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
      </CardContent>
    </Card>
  );
}
