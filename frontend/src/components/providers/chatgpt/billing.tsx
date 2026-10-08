"use client";

import { useId, useState } from "react";
import { RefreshCw } from "lucide-react";
import { request } from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useActions, useErrorToast } from "@/lib/actions";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";

type SearchPrice = { price: string | null; revision: number | null };

export function SearchBilling() {
  const id = useId();
  const resource = useResource<SearchPrice>("/billing/chatgpt/search");
  const [draft, setDraft] = useState<SearchPrice>();
  const actions = useActions();
  const busy = actions.isBusy("search-price");
  useErrorToast(resource.error);
  const price = draft?.price ?? resource.data?.price ?? "";

  return (
    <Card>
      <CardContent>
        <form
          noValidate
          className="flex flex-wrap items-end gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            if (!resource.ready || !draft || busy) return;
            void actions.run(
              "search-price",
              async () => {
                await request<SearchPrice>("/billing/chatgpt/search", {
                  method: "PUT",
                  body: { ...draft, price: price.trim() || null },
                });
                setDraft(undefined);
                resource.reload();
              },
              { success: "搜索价格已保存" },
            );
          }}
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
