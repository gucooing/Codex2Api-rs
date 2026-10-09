"use client";
import { ClientState } from "@/components/platform-account/client-state/client-state";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useClientStatePanel } from "@/lib/platform-account/client-state";

export function ClientStatePanel({ id }: { id: string }) {
  const { fieldId, setKey, items, selected } = useClientStatePanel();
  return (
    <div className="min-w-0 space-y-3">
      <Field orientation="horizontal" className="w-fit flex-wrap">
        <FieldLabel htmlFor={`${fieldId}-state`}>状态类别</FieldLabel>
        <Select value={selected?.key} onValueChange={setKey}>
          <SelectTrigger id={`${fieldId}-state`} className="w-64">
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            {items.map((item) => (
              <SelectItem key={item.key} value={item.key}>
                {item.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      {selected && <ClientState key={`${id}:${selected.key}`} id={id} config={selected} />}
    </div>
  );
}
