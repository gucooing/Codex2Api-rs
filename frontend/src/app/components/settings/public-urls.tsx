"use client";
import { fields, usePublicUrls } from "@/app/data/settings";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";

export function PublicUrls() {
  const { fieldId, resource, setDraft, current, busy, disabled, handleSubmit } = usePublicUrls();
  return (
    <form
      noValidate
      className="space-y-4"
      aria-busy={busy || resource.refreshing}
      onSubmit={(event) => handleSubmit(event)}
    >
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={resource.reload}
          disabled={resource.refreshing || busy}
        >
          {resource.refreshing && <Spinner />}刷新
        </Button>
        <Button type="submit" size="sm" disabled={disabled}>
          {busy && <Spinner />}
          {busy ? "正在保存…" : "保存"}
        </Button>
      </div>
      <Card>
        <CardContent>
          <FieldSet disabled={disabled}>
            <FieldGroup className="gap-4">
              {fields.map((field) => (
                <Field key={field.key}>
                  <FieldLabel htmlFor={`${fieldId}-${field.key}`}>{field.label}</FieldLabel>
                  <Input
                    id={`${fieldId}-${field.key}`}
                    type="url"
                    required
                    autoComplete="off"
                    spellCheck={false}
                    value={current?.[field.key] ?? ""}
                    onChange={(event) => {
                      if (current) setDraft({ ...current, [field.key]: event.target.value });
                    }}
                  />
                </Field>
              ))}
            </FieldGroup>
          </FieldSet>
        </CardContent>
      </Card>
    </form>
  );
}
