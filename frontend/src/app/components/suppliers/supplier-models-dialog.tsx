"use client";
import { useSupplierModelsBatch } from "@/app/data/suppliers";
import { Button } from "@/components/ui/button";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { SupplierSelection } from "@/lib/supplier-selection";
import { useId } from "react";

export function SupplierModelsDialog({
  accounts,
  onClose,
  onSaved,
}: {
  accounts: SupplierSelection[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const id = useId();
  const state = useSupplierModelsBatch(accounts, onSaved);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !state.busy) onClose();
      }}
    >
      <DialogContent
        className="flex h-[40rem] max-h-[calc(100%-2rem)] flex-col overflow-hidden sm:max-w-xl"
        onInteractOutside={(event) => {
          if (state.busy) event.preventDefault();
        }}
        onEscapeKeyDown={(event) => {
          if (state.busy) event.preventDefault();
        }}
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>更新模型</DialogTitle>
          <DialogDescription>覆盖 {accounts.length} 个账户的全部可用模型。</DialogDescription>
        </DialogHeader>
        <form noValidate onSubmit={state.submit} className="flex min-h-0 flex-1 flex-col gap-4">
          <Field className="shrink-0">
            <FieldLabel htmlFor={id}>可用模型</FieldLabel>
            <Combobox
              multiple
              items={state.choices}
              value={state.models}
              inputValue={state.search}
              onValueChange={state.select}
              filter={null}
              onInputValueChange={(text, details) => {
                if (details.reason === "input-change" || details.reason === "input-clear")
                  state.setSearch(text);
              }}
            >
              <ComboboxInput id={id} placeholder="选择或输入模型名称" disabled={state.busy} />
              <ComboboxContent className="pointer-events-auto">
                <ComboboxEmpty>{state.options.loading ? "加载中…" : "没有匹配模型"}</ComboboxEmpty>
                <ComboboxList>
                  {(model) => (
                    <ComboboxItem key={model} value={model}>
                      {model}
                    </ComboboxItem>
                  )}
                </ComboboxList>
              </ComboboxContent>
            </Combobox>
          </Field>
          <p className="shrink-0 text-sm text-muted-foreground">
            已选 {state.models.length} 个模型
          </p>
          <ScrollArea className="min-h-0 flex-1">
            <Table aria-label="已选模型" className="table-fixed">
              <TableHeader>
                <TableRow>
                  <TableHead>模型</TableHead>
                  <TableHead className="w-20 text-right">操作</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {state.models.map((model) => (
                  <TableRow key={model}>
                    <TableCell className="whitespace-normal break-all">{model}</TableCell>
                    <TableCell className="text-right">
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={state.busy}
                        onClick={() => state.remove(model)}
                      >
                        移除
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
                {state.models.length === 0 && (
                  <TableRow>
                    <TableCell colSpan={2} className="text-muted-foreground">
                      暂未选择模型
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </ScrollArea>
          <DialogFooter className="shrink-0">
            <Button type="button" variant="outline" disabled={state.busy} onClick={onClose}>
              取消
            </Button>
            <Button type="submit" disabled={state.busy || !state.options.ready || !state.changed}>
              {state.busy ? "正在更新…" : "覆盖更新"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
