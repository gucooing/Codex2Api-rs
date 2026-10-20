"use client";
import { Button } from "@/components/ui/button";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import { Field, FieldLabel } from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Download } from "lucide-react";

/** The model list a new API-key account serves: models its endpoint lists, catalog
 * models and custom names. */
export function ApiAccountModels({
  id,
  models,
  choices,
  search,
  loading,
  fetching,
  disabled,
  onSearch,
  onChange,
  onRemove,
  onFetch,
}: {
  id: string;
  models: string[];
  choices: string[];
  search: string;
  loading: boolean;
  fetching: boolean;
  disabled: boolean;
  onSearch: (search: string) => void;
  onChange: (models: string[]) => void;
  onRemove: (model: string) => void;
  onFetch: () => void;
}) {
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-end gap-2">
        <Field className="w-full sm:w-80">
          <FieldLabel htmlFor={id}>添加模型</FieldLabel>
          <Combobox
            multiple
            items={choices}
            value={models}
            inputValue={search}
            onValueChange={onChange}
            filter={null}
            onInputValueChange={(text, details) => {
              if (details.reason === "input-change" || details.reason === "input-clear")
                onSearch(text);
            }}
          >
            <ComboboxInput id={id} placeholder="选择或输入模型名称" disabled={disabled} />
            <ComboboxContent>
              <ComboboxEmpty>{loading ? "加载中…" : "没有匹配模型"}</ComboboxEmpty>
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
        <Button type="button" variant="outline" disabled={disabled} onClick={onFetch}>
          {fetching ? <Spinner /> : <Download />}
          获取上游模型
        </Button>
      </div>
      <p className="text-sm text-muted-foreground">已选 {models.length} 个模型</p>
      <Table aria-label="已选模型" className="table-fixed">
        <TableHeader>
          <TableRow>
            <TableHead>模型</TableHead>
            <TableHead className="w-20 text-right">操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {models.map((model) => (
            <TableRow key={model}>
              <TableCell className="whitespace-normal break-all">{model}</TableCell>
              <TableCell className="text-right">
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={disabled}
                  onClick={() => onRemove(model)}
                >
                  移除
                </Button>
              </TableCell>
            </TableRow>
          ))}
          {models.length === 0 && (
            <TableRow>
              <TableCell colSpan={2} className="text-muted-foreground">
                暂未选择模型
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </div>
  );
}
