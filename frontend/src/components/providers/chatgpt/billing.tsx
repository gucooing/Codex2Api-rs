"use client";
import { useSearchBilling } from "@/lib/providers/chatgpt/data";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { RefreshCw } from "lucide-react";

export function SearchBilling() {
  const { id, resource, draft, setDraft, busy, price, handleSubmit } = useSearchBilling();
  return (
    <Card>
      <CardContent>
        <form
          noValidate
          className="flex flex-wrap items-end gap-3"
          onSubmit={(event) => handleSubmit(event)}
        >
          <Field className="w-32">
            <FieldLabel>提供商</FieldLabel>
            <p className="text-sm">ChatGPT</p>
          </Field>
          <Field className="w-32">
            <FieldLabel>计费项目</FieldLabel>
            <p className="text-sm">搜索</p>
          </Field>
          <Field className="w-24">
            <FieldLabel>计费方式</FieldLabel>
            <p className="text-sm">按次</p>
          </Field>
          <Field className="w-48">
            <FieldLabel htmlFor={id}>每次价格（USD）</FieldLabel>
            <Input
              id={id}
              inputMode="decimal"
              value={price}
              placeholder="未配置"
              disabled={!resource.ready || busy}
              onChange={(event) => {
                if (resource.data)
                  setDraft({
                    price: event.target.value,
                    revision: draft ? draft.revision : resource.data.revision,
                  });
              }}
            />
          </Field>
          <div className="flex items-center gap-2 xl:ml-auto">
            <Button
              type="button"
              variant="outline"
              disabled={busy || resource.refreshing}
              onClick={() => {
                setDraft(undefined);
                resource.reload();
              }}
            >
              <RefreshCw />
              刷新
            </Button>
            <Button type="submit" disabled={!resource.ready || !draft || busy}>
              保存
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
