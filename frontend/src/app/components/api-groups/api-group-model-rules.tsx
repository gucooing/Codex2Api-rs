"use client";
import type { useApiGroupEditor } from "@/app/data/api-groups";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldDescription, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
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

export function ApiGroupModelRules({ editor }: { editor: ReturnType<typeof useApiGroupEditor> }) {
  const {
    id,
    value,
    modelMode,
    setModelMode,
    modelSearch,
    setModelSearch,
    models,
    rules,
    setRule,
    toggleModel,
    selectVisible,
    busy,
  } = editor;
  return (
    <FieldSet disabled={busy} className="gap-3">
      <div className="flex flex-wrap items-end gap-3">
        <Field className="w-56">
          <FieldLabel htmlFor={id + "-model-mode"}>可用模型</FieldLabel>
          <Select value={modelMode} onValueChange={setModelMode}>
            <SelectTrigger id={id + "-model-mode"} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="all">标签支持的全部模型</SelectItem>
              <SelectItem value="selected">指定模型</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Field className="w-48">
          <FieldLabel htmlFor={id + "-model-search"}>搜索模型</FieldLabel>
          <Input
            id={id + "-model-search"}
            placeholder="模型名称"
            value={modelSearch}
            onChange={(event) => setModelSearch(event.target.value)}
          />
        </Field>
        <Button
          type="button"
          variant="outline"
          disabled={!models.ready || modelMode === "all"}
          onClick={() => selectVisible(true)}
        >
          选择当前结果
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={!models.ready || modelMode === "all"}
          onClick={() => selectVisible(false)}
        >
          清除当前结果
        </Button>
      </div>
      <FieldDescription>
        模型倍率留空继承组倍率；每用户组限制与模型限制同时生效，0 表示该项无限。
      </FieldDescription>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>可用</TableHead>
            <TableHead>模型</TableHead>
            <TableHead>模型倍率</TableHead>
            <TableHead>每用户模型并发</TableHead>
            <TableHead>每用户模型 RPM</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {models.data?.items.map((model) => {
            const rule = rules.find((row) => row.model === model.model);
            const selected = modelMode === "all" || value.allowed_models.includes(model.model);
            return (
              <TableRow key={model.model}>
                <TableCell>
                  <Checkbox
                    aria-label={"启用模型 " + model.model}
                    checked={selected}
                    disabled={modelMode === "all" || !models.ready}
                    onCheckedChange={(checked) => toggleModel(model.model, checked === true)}
                  />
                </TableCell>
                <TableCell className="max-w-56 truncate" title={model.model}>
                  {model.model}
                </TableCell>
                <TableCell>
                  <Input
                    className="min-w-24"
                    aria-label={model.model + " 倍率"}
                    inputMode="decimal"
                    placeholder={"继承 " + value.multiplier + "×"}
                    value={rule?.multiplier ?? ""}
                    disabled={!models.ready}
                    onChange={(event) => setRule(model.model, "multiplier", event.target.value)}
                  />
                </TableCell>
                <TableCell>
                  <Input
                    className="min-w-24"
                    aria-label={model.model + " 并发"}
                    inputMode="numeric"
                    value={rule?.concurrency_limit ?? "0"}
                    disabled={!models.ready}
                    onChange={(event) =>
                      setRule(model.model, "concurrency_limit", event.target.value)
                    }
                  />
                </TableCell>
                <TableCell>
                  <Input
                    className="min-w-24"
                    aria-label={model.model + " RPM"}
                    inputMode="numeric"
                    value={rule?.rpm_limit ?? "0"}
                    disabled={!models.ready}
                    onChange={(event) => setRule(model.model, "rpm_limit", event.target.value)}
                  />
                </TableCell>
              </TableRow>
            );
          })}
          {models.ready && !models.data?.items.length && (
            <TableRow>
              <TableCell colSpan={5} className="text-center text-muted-foreground">
                暂无匹配模型
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </FieldSet>
  );
}
