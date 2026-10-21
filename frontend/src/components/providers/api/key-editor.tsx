"use client";
import { useApiKeyEditor } from "@/app/data/suppliers/api-accounts";
import { Button } from "@/components/ui/button";
import { FieldSet } from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";
import { ApiKeyField } from "./key-field";

export function ApiKeyEditor(props: { id: string; disabled: boolean; onSaved: () => void }) {
  const { fieldId, resource, value, setDraft, ready, changed, busy, submit } =
    useApiKeyEditor(props);
  return (
    <form noValidate onSubmit={submit} className="space-y-3" aria-busy={busy}>
      <FieldSet disabled={!ready || busy}>
        <ApiKeyField id={`${fieldId}-key`} value={value} onChange={setDraft} />
      </FieldSet>
      <div className="flex flex-wrap gap-2">
        <Button type="submit" disabled={!ready || !changed || busy}>
          {busy && <Spinner />}
          保存 Key
        </Button>
        {resource.error && (
          <Button type="button" variant="outline" onClick={resource.reload}>
            重新加载 Key
          </Button>
        )}
      </div>
    </form>
  );
}
