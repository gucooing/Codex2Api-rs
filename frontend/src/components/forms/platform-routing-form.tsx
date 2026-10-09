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
import { Field, FieldDescription, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { type Consumer, type Supplier } from "@/lib/api";
import { RoutingResponse, useRoutingForm } from "@/lib/platform-account";

export function RoutingForm({
  account,
  data,
  disabled,
  onSaved,
}: {
  account?: Consumer;
  data: RoutingResponse;
  disabled: boolean;
  onSaved: () => void;
}) {
  const {
    id,
    actions,
    rpm,
    setDraft,
    current,
    setRpm,
    rpmValue,
    setSupplierSearch,
    supplierOpen,
    setSupplierOpen,
    tags,
    suppliers,
    assigned,
    handleSubmit,
    handleSubmit2,
  } = useRoutingForm({ account, data, disabled, onSaved });
  return (
    <div className="space-y-4">
      <form noValidate className="space-y-3" onSubmit={(event) => handleSubmit(event)}>
        <FieldSet disabled={disabled || !tags.ready || actions.isBusy("pool-route")}>
          <Field>
            <FieldLabel htmlFor={`${id}-tag`}>标签号池</FieldLabel>
            <Select
              value={current.tag || "none"}
              onValueChange={(tag) => {
                if (disabled || !tags.ready || !tag || tag === (current.tag || "none")) return;
                setDraft({ tag: tag === "none" ? "" : tag, supplier: "" });
              }}
            >
              <SelectTrigger id={`${id}-tag`}>
                <SelectValue placeholder="选择标签号池" />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="none">不绑定号池</SelectItem>
                {(tags.data?.items ?? []).map((t) => (
                  <SelectItem key={t.id} value={t.id}>
                    {t.name}（{t.supplier_count} 个供应账户）
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor={`${id}-supplier`}>分配账户</FieldLabel>
            <Combobox<Supplier>
              items={suppliers.data?.items ?? []}
              value={assigned}
              disabled={!current.tag || disabled}
              filter={null}
              open={supplierOpen}
              onOpenChange={(open) => {
                setSupplierOpen(open);
                if (open) setSupplierSearch("");
              }}
              itemToStringLabel={(item) => item.email || item.display_name || item.id}
              itemToStringValue={(item) => item.id}
              isItemEqualToValue={(item, value) => item.id === value.id}
              onInputValueChange={(text, details) => {
                if (details.reason === "input-change") setSupplierSearch(text);
              }}
              onValueChange={(item) => {
                if (item) setDraft({ ...current, supplier: item.id });
              }}
            >
              <ComboboxInput
                id={`${id}-supplier`}
                placeholder={current.supplier ? "加载中…" : "暂未分配"}
              />
              <ComboboxContent>
                <ComboboxEmpty>{suppliers.loading ? "加载中…" : "没有匹配账户"}</ComboboxEmpty>
                <ComboboxList>
                  {(item: Supplier) => (
                    <ComboboxItem key={item.id} value={item}>
                      {item.email || item.display_name || item.id}
                    </ComboboxItem>
                  )}
                </ComboboxList>
              </ComboboxContent>
            </Combobox>
          </Field>
          <Button type="submit">保存绑定</Button>
        </FieldSet>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => {
            onSaved();
            tags.reload();
            suppliers.reload();
            rpm.reload();
          }}
        >
          刷新
        </Button>
      </form>
      <form noValidate className="space-y-3" onSubmit={(event) => handleSubmit2(event)}>
        <FieldSet disabled={!rpm.ready || actions.isBusy("consumer-rpm")}>
          <Field>
            <FieldLabel htmlFor={`${id}-rpm`}>RPM 限制</FieldLabel>
            <Input
              id={`${id}-rpm`}
              type="number"
              min={0}
              step={1}
              value={rpmValue}
              placeholder="继承默认限制"
              onChange={(event) => setRpm(event.target.value)}
            />
            <FieldDescription>
              留空用默认值（{rpm.data?.default_rpm ?? "—"}），0 为无限。
            </FieldDescription>
          </Field>
          <Button type="submit">保存 RPM</Button>
        </FieldSet>
      </form>
    </div>
  );
}
