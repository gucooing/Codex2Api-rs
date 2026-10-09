"use client";
import { useSupplierTagEditor } from "@/app/data/suppliers";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldDescription, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  supplierTagChecked,
  toggleSupplierSelection,
  type SupplierSelection,
} from "@/lib/supplier-selection";
import Link from "next/link";

export function SupplierTagEditor({
  accounts,
  disabled,
  batch = false,
  onSaved,
}: {
  accounts: SupplierSelection[];
  disabled: boolean;
  batch?: boolean;
  onSaved: () => void;
}) {
  const {
    fieldId,
    draft,
    setDraft,
    provider,
    tags,
    choices,
    common,
    changed,
    busy,
    ready,
    handleSubmit,
  } = useSupplierTagEditor({ accounts, disabled, batch, onSaved });
  return (
    <form noValidate className="space-y-4" onSubmit={(event) => handleSubmit(event)}>
      <FieldSet disabled={!ready || busy}>
        <FieldLegend>
          所属标签
          {provider
            ? ` · ${provider === "chatgpt" ? "ChatGPT" : provider === "grok" ? "Grok" : provider}`
            : ""}
        </FieldLegend>
        {!provider && accounts.length > 0 && (
          <FieldDescription>请选择同一平台的账户。</FieldDescription>
        )}
        <ScrollArea className="[&>[data-slot=scroll-area-viewport]]:max-h-72">
          <div className="grid gap-2 sm:grid-cols-2">
            {choices.map((tag) => (
              <Field key={tag.id} orientation="horizontal">
                <Checkbox
                  id={`${fieldId}-${tag.id}`}
                  checked={
                    draft === undefined
                      ? supplierTagChecked(accounts, tag.id)
                      : draft.includes(tag.id)
                  }
                  onCheckedChange={(checked) =>
                    setDraft(toggleSupplierSelection(draft ?? common, [tag.id], checked === true))
                  }
                />
                <FieldLabel htmlFor={`${fieldId}-${tag.id}`}>{tag.name}</FieldLabel>
              </Field>
            ))}
          </div>
        </ScrollArea>
        {tags.ready && provider && choices.length === 0 && (
          <FieldDescription>当前平台暂无标签。</FieldDescription>
        )}
        <div className="flex flex-wrap gap-2">
          <Button type="button" variant="outline" size="sm" onClick={() => setDraft([])}>
            清空勾选
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={draft === undefined || busy}
            onClick={() => setDraft(undefined)}
          >
            撤销修改
          </Button>
        </div>
      </FieldSet>
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          type="button"
          variant="ghost"
          onClick={tags.reload}
          disabled={busy || tags.refreshing}
        >
          刷新标签
        </Button>
        {!batch && (
          <Button asChild type="button" variant="outline">
            <Link href="/supplier-tags/">前往标签管理</Link>
          </Button>
        )}
        <Button type="submit" disabled={!ready || busy || !changed}>
          {busy ? "正在更新…" : "更新标签"}
        </Button>
      </div>
    </form>
  );
}
