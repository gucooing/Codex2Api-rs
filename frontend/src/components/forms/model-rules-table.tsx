"use client";
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
import type { ModelRuleDraft, ModelRuleRow } from "@/lib/model-rules";
import { ModelPlatformIcon } from "@/components/model-platforms";
import { platformLabel, platformOptions } from "@/lib/model-platforms";

/** Manual model grants with per-model multiplier and per-user limits. Rows keep the
 * catalog order whether or not they are checked; the search and platform narrow the
 * catalog candidates, and a granted model the catalog no longer offers stays listed
 * after them while it matches the search and no platform is chosen. */
export function ModelRulesTable({
  id,
  rows,
  ready,
  rules,
  baseMultiplier,
  search,
  onSearch,
  platform,
  onPlatform,
  onToggle,
  onSelectVisible,
  onRule,
  disabled,
}: {
  id: string;
  rows: ModelRuleRow[];
  ready: boolean;
  rules: ModelRuleDraft[];
  baseMultiplier: string;
  search: string;
  onSearch: (value: string) => void;
  platform: string;
  onPlatform: (value: string) => void;
  onToggle: (model: string, checked: boolean) => void;
  onSelectVisible: (checked: boolean) => void;
  onRule: (model: string, field: keyof Omit<ModelRuleDraft, "model">, value: string) => void;
  disabled: boolean;
}) {
  return (
    <FieldSet disabled={disabled} className="gap-3">
      <div className="flex flex-wrap items-end gap-3">
        <Field className="w-48">
          <FieldLabel htmlFor={id + "-model-search"}>搜索模型</FieldLabel>
          <Input
            id={id + "-model-search"}
            placeholder="模型名称"
            value={search}
            onChange={(event) => onSearch(event.target.value)}
          />
        </Field>
        <Field className="w-36">
          <FieldLabel htmlFor={id + "-model-platform"}>平台</FieldLabel>
          <Select
            value={platform || "all"}
            onValueChange={(next) => onPlatform(next === "all" ? "" : next)}
          >
            <SelectTrigger id={id + "-model-platform"} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="all">全部平台</SelectItem>
              {platformOptions.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  <ModelPlatformIcon platform={option.value} />
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Button
          type="button"
          variant="outline"
          disabled={!ready}
          onClick={() => onSelectVisible(true)}
        >
          选择当前结果
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={!ready}
          onClick={() => onSelectVisible(false)}
        >
          清除当前结果
        </Button>
        <span className="ml-auto text-xs text-muted-foreground">已选择 {rules.length} 个模型</span>
      </div>
      <FieldDescription>
        仅勾选的模型可用。模型倍率留空继承基础倍率；每用户并发与 RPM 为 0 表示该项无限。
      </FieldDescription>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>可用</TableHead>
            <TableHead>模型</TableHead>
            <TableHead>平台</TableHead>
            <TableHead>模型倍率</TableHead>
            <TableHead>每用户模型并发</TableHead>
            <TableHead>每用户模型 RPM</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row) => {
            const rule = rules.find((item) => item.model === row.model);
            return (
              <TableRow key={row.model}>
                <TableCell>
                  <Checkbox
                    aria-label={"启用模型 " + row.model}
                    checked={!!rule}
                    disabled={!ready}
                    onCheckedChange={(checked) => onToggle(row.model, checked === true)}
                  />
                </TableCell>
                <TableCell className="max-w-56 truncate" title={row.model}>
                  {row.model}
                  {!row.enabled && (
                    <span className="ml-2 text-xs text-muted-foreground">已停用</span>
                  )}
                </TableCell>
                <TableCell>
                  {row.platform ? (
                    <span className="flex items-center gap-1">
                      <ModelPlatformIcon platform={row.platform} className="size-3.5" />
                      {platformLabel(row.platform)}
                    </span>
                  ) : (
                    "—"
                  )}
                </TableCell>
                <TableCell>
                  <Input
                    className="min-w-24"
                    aria-label={row.model + " 倍率"}
                    inputMode="decimal"
                    placeholder={"继承 " + baseMultiplier + "×"}
                    value={rule?.multiplier ?? ""}
                    disabled={!ready || !rule}
                    onChange={(event) => onRule(row.model, "multiplier", event.target.value)}
                  />
                </TableCell>
                <TableCell>
                  <Input
                    className="min-w-24"
                    aria-label={row.model + " 并发"}
                    inputMode="numeric"
                    value={rule?.concurrency_limit ?? "0"}
                    disabled={!ready || !rule}
                    onChange={(event) => onRule(row.model, "concurrency_limit", event.target.value)}
                  />
                </TableCell>
                <TableCell>
                  <Input
                    className="min-w-24"
                    aria-label={row.model + " RPM"}
                    inputMode="numeric"
                    value={rule?.rpm_limit ?? "0"}
                    disabled={!ready || !rule}
                    onChange={(event) => onRule(row.model, "rpm_limit", event.target.value)}
                  />
                </TableCell>
              </TableRow>
            );
          })}
          {ready && !rows.length && (
            <TableRow>
              <TableCell colSpan={6} className="text-center text-muted-foreground">
                暂无匹配模型
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </FieldSet>
  );
}
