"use client";
import { useSupplierTransfer } from "@/app/data/suppliers/transfer";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useId } from "react";

export function SupplierTransferDialog({
  mode,
  ids,
  onClose,
  onSaved,
}: {
  mode: "import" | "export";
  ids: string[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const id = useId();
  const state = useSupplierTransfer(mode, ids, onSaved);
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
          <DialogTitle>{mode === "export" ? "导出供应账户" : "导入供应账户"}</DialogTitle>
          <DialogDescription>
            {mode === "export"
              ? `已选择 ${ids.length} 个账户，导出为 JSON 文件。`
              : "选择 JSON 文件；不支持的平台和已存在的账户会跳过。"}
          </DialogDescription>
        </DialogHeader>
        <form noValidate onSubmit={state.submit} className="flex min-h-0 flex-1 flex-col gap-4">
          <ScrollArea className="min-h-0 flex-1">
            <div className="space-y-4 pr-3">
              <Field>
                <FieldLabel htmlFor={`${id}-format`}>
                  {mode === "export" ? "导出格式" : "导入格式"}
                </FieldLabel>
                <Select value={state.format} onValueChange={state.setFormat} disabled={state.busy}>
                  <SelectTrigger id={`${id}-format`} className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="native">
                      {mode === "export" ? "直接导出" : "直接导入"}
                    </SelectItem>
                    <SelectItem value="sub2api">Sub2api 格式</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
              {mode === "import" && (
                <Field>
                  <FieldLabel htmlFor={`${id}-file`}>JSON 文件</FieldLabel>
                  <Input
                    id={`${id}-file`}
                    type="file"
                    accept=".json,application/json"
                    disabled={state.busy}
                    onChange={(event) => state.selectFile(event.target.files?.[0] ?? null)}
                  />
                </Field>
              )}
              <Field orientation="horizontal">
                <Checkbox
                  id={`${id}-proxy`}
                  checked={state.includeProxies}
                  disabled={state.busy}
                  onCheckedChange={(checked) => state.setIncludeProxies(checked === true)}
                />
                <FieldLabel htmlFor={`${id}-proxy`}>
                  {mode === "export" ? "导出代理" : "导入代理"}
                </FieldLabel>
              </Field>
              {state.resultRows && (
                <>
                  {mode === "export" ? (
                    <p className="text-sm">已导出 {state.exportRows?.length} 个账户</p>
                  ) : (
                    state.result && (
                      <p className="text-sm">
                        导入 {state.result.imported} 个，跳过 {state.result.skipped} 个，失败{" "}
                        {state.result.failed} 个
                      </p>
                    )
                  )}
                  <Table className="table-fixed">
                    <TableHeader>
                      <TableRow>
                        <TableHead>名称</TableHead>
                        <TableHead className="text-right">结果</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {state.resultRows.map((row) => (
                        <TableRow key={row.row}>
                          <TableCell className="whitespace-normal break-all">
                            {row.name || "—"}
                          </TableCell>
                          <TableCell className="text-right whitespace-normal break-words">
                            {row.message}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </>
              )}
            </div>
          </ScrollArea>
          <DialogFooter className="shrink-0">
            <Button type="button" variant="outline" disabled={state.busy} onClick={onClose}>
              关闭
            </Button>
            <Button type="submit" disabled={state.busy || (mode === "import" && !state.file)}>
              {state.busy ? "正在处理…" : mode === "export" ? "导出 JSON" : "导入"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
