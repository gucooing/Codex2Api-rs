"use client";
import { useId, useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Field, FieldLabel, FieldDescription, FieldSet, FieldLegend } from "@/components/ui/field";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type Supplier, type SupplierTag, type List } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import {
  commonSupplierTags,
  selectedSupplierProvider,
  supplierTagChecked,
  toggleSupplierSelection,
} from "@/lib/supplier-selection";

export function SupplierTagEditor({
  accounts,
  disabled,
  batch = false,
  onSaved,
}: {
  accounts: Supplier[];
  disabled: boolean;
  batch?: boolean;
  onSaved: () => void;
}) {
  const fieldId = useId();
  const tags = useResource<List<SupplierTag>>("/supplier-tags");
  const [draft, setDraft] = useState<string[]>();
  const actions = useActions();
  const provider = selectedSupplierProvider(accounts);
  const choices = (tags.data?.items ?? []).filter((tag) => tag.provider_id === provider);
  const common = commonSupplierTags(accounts);
  const changed =
    draft !== undefined &&
    accounts.some(
      (account) =>
        account.tag_ids.length !== draft.length ||
        account.tag_ids.some((tag) => !draft.includes(tag)),
    );
  const key = batch ? "supplier-tags-batch" : "supplier-tags-single";
  const busy = actions.isBusy(key);
  const ready = !disabled && tags.ready && accounts.length > 0 && provider !== undefined;
  useErrorToast(tags.error);
  return (
    <form
      noValidate
      className="space-y-4"
      onSubmit={(event) =>
        actions.submit(
          event,
          key,
          async () => {
            if (!ready || draft === undefined || !changed)
              throw new Error("请加载账户资料并修改标签后再保存");
            await request("/suppliers/tags", {
              method: "POST",
              body: { account_ids: accounts.map((account) => account.id), tag_ids: draft },
            });
            setDraft(undefined);
            onSaved();
          },
          "标签已更新",
        )
      }
    >
      <FieldSet disabled={!ready || busy}>
        <FieldLegend>
          所属标签
          {provider
            ? ` · ${provider === "chatgpt" ? "ChatGPT" : provider === "grok" ? "Grok" : provider}`
            : ""}
        </FieldLegend>
        {batch && (
          <FieldDescription>
            修改后按勾选结果覆盖所选账户的标签。横线表示仅部分账户使用。
          </FieldDescription>
        )}
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

export function SupplierTagsBatchDialog({
  open,
  onOpenChange,
  accounts,
  disabled,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  accounts: Supplier[];
  disabled: boolean;
  onSaved: () => void;
}) {
  const actions = useActions();
  const busy = actions.isBusy("supplier-tags-batch");
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy) onOpenChange(next);
      }}
    >
      <DialogContent
        className="sm:max-w-xl"
        onInteractOutside={(event) => {
          if (busy) event.preventDefault();
        }}
        onEscapeKeyDown={(event) => {
          if (busy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>批量更新标签</DialogTitle>
          <DialogDescription>已选择 {accounts.length} 个供应账户。</DialogDescription>
        </DialogHeader>
        <SupplierTagEditor
          key={accounts.map((account) => account.id).join(",")}
          accounts={accounts}
          disabled={disabled}
          batch
          onSaved={onSaved}
        />
      </DialogContent>
    </Dialog>
  );
}
